use std::cmp::Ordering;
use std::convert::TryFrom;
use std::env;
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortMode {
    Name,
    Time,
    Size,
    Unsorted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[cfg(unix)]
type PlatformTime = std::os::raw::c_long;

#[cfg(windows)]
type PlatformTime = i64;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    #[cfg(unix)]
    tm_gmtoff: std::os::raw::c_long,
    #[cfg(unix)]
    tm_zone: *const std::os::raw::c_char,
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(timep: *const PlatformTime, result: *mut NativeTm) -> *mut NativeTm;
    fn gmtime_r(timep: *const PlatformTime, result: *mut NativeTm) -> *mut NativeTm;
}

#[cfg(windows)]
unsafe extern "C" {
    fn _localtime64_s(result: *mut NativeTm, timep: *const PlatformTime) -> i32;
    fn _gmtime64_s(result: *mut NativeTm, timep: *const PlatformTime) -> i32;
}

#[derive(Debug, Default)]
struct Config {
    show_all: bool,
    almost_all: bool,
    long: bool,
    one_per_line: bool,
    recursive: bool,
    reverse: bool,
    directory_only: bool,
    human_readable: bool,
    classify: bool,
    count_children: bool,
    sort_mode: SortMode,
    color: ColorMode,
    paths: Vec<PathBuf>,
}

impl Default for SortMode {
    fn default() -> Self {
        Self::Name
    }
}

impl Default for ColorMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug)]
struct EntryInfo {
    name: OsString,
    path: PathBuf,
    metadata: Metadata,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("ls-rs: {err}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, String> {
    let mut config = parse_args(env::args().skip(1))?;
    let paths = if config.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        std::mem::take(&mut config.paths)
    };

    let show_headers = config.recursive || config.count_children || paths.len() > 1;
    let mut exit_code = 0;
    let mut first_section = true;

    for path in &paths {
        if let Err(err) = list_path(path, &config, show_headers, &mut first_section) {
            eprintln!("ls-rs: {}: {err}", path.display());
            exit_code = 1;
        }
    }

    Ok(exit_code)
}

fn parse_args<I>(args: I) -> Result<Config, String>
where
    I: IntoIterator<Item = String>,
{
    let mut config = Config::default();
    let mut parsing_flags = true;

    for arg in args {
        if parsing_flags && arg == "--" {
            parsing_flags = false;
            continue;
        }

        if parsing_flags && arg.starts_with("--") {
            match arg.as_str() {
                "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "--version" => {
                    println!("ls-rs {VERSION}");
                    std::process::exit(0);
                }
                "--human-readable" => config.human_readable = true,
                "--recursive" => config.recursive = true,
                "--classify" => config.classify = true,
                "--count-children" => config.count_children = true,
                "--reverse" => config.reverse = true,
                "--unsorted" => config.sort_mode = SortMode::Unsorted,
                "--sort=time" => config.sort_mode = SortMode::Time,
                "--sort=size" => config.sort_mode = SortMode::Size,
                "--sort=name" => config.sort_mode = SortMode::Name,
                "--color=auto" => config.color = ColorMode::Auto,
                "--color=always" => config.color = ColorMode::Always,
                "--color=never" => config.color = ColorMode::Never,
                _ if arg.starts_with("--color=") => {
                    return Err(format!("unknown color mode '{arg}'"));
                }
                _ => return Err(format!("unknown option '{arg}'")),
            }
        } else if parsing_flags && arg.starts_with('-') && arg.len() > 1 {
            for flag in arg.chars().skip(1) {
                match flag {
                    'a' => config.show_all = true,
                    'A' => config.almost_all = true,
                    'l' => config.long = true,
                    '1' => config.one_per_line = true,
                    'R' => config.recursive = true,
                    'r' => config.reverse = true,
                    'd' => config.directory_only = true,
                    'F' => config.classify = true,
                    'c' => config.count_children = true,
                    'H' => config.human_readable = true,
                    't' => config.sort_mode = SortMode::Time,
                    'S' => config.sort_mode = SortMode::Size,
                    'U' => config.sort_mode = SortMode::Unsorted,
                    'h' => {
                        print_help();
                        std::process::exit(0);
                    }
                    _ => return Err(format!("unknown option '-{flag}'")),
                }
            }
        } else {
            config.paths.push(PathBuf::from(arg));
        }
    }

    if config.long {
        config.one_per_line = true;
    }

    Ok(config)
}

fn print_help() {
    println!(
        "ls-rs {VERSION}\n\n\
Usage: ls-rs [OPTIONS] [PATH...]\n\n\
Options:\n\
  -a                show hidden entries, including . and ..\n\
  -A                show hidden entries except . and ..\n\
  -l                use a long listing format\n\
  -1                print one entry per line\n\
  -R, --recursive   recurse into subdirectories\n\
  -r, --reverse     reverse sort order\n\
  -d                list directories themselves, not their contents\n\
  -F, --classify    append /, *, or @ where useful\n\
  -c, --count-children  show direct child counts for directories\n\
  -H, --human-readable  show sizes using human-friendly units\n\
  -t, --sort=time   sort by modification time\n\
  -S, --sort=size   sort by file size\n\
  --sort=name       sort by name\n\
  -U, --unsorted    keep directory order\n\
  --color=auto|always|never  control color output\n\
  -h, --help        show this help\n\
  --version         show version\n"
    );
}

fn list_path(
    path: &Path,
    config: &Config,
    show_header: bool,
    first_section: &mut bool,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|err| err.to_string())?;

    if metadata.is_dir() && !config.directory_only {
        list_directory(path, path, config, show_header, first_section)
    } else {
        let entry = EntryInfo {
            name: display_name_from_path(path),
            path: path.to_path_buf(),
            metadata,
        };
        print_section(None, &[entry], config, show_header, first_section)?;
        Ok(())
    }
}

fn list_directory(
    path: &Path,
    display_path: &Path,
    config: &Config,
    show_header: bool,
    first_section: &mut bool,
) -> Result<(), String> {
    let mut entries = read_dir(path, config)?;
    sort_entries(&mut entries, config);

    print_section(
        Some(display_path),
        &entries,
        config,
        show_header,
        first_section,
    )?;

    if config.recursive {
        for entry in entries {
            if is_directory_entry(&entry) && !is_dot_entry(&entry.name) {
                list_directory(&entry.path, &entry.path, config, true, first_section)?;
            }
        }
    }

    Ok(())
}

fn read_dir(path: &Path, config: &Config) -> Result<Vec<EntryInfo>, String> {
    let mut entries = Vec::new();

    if config.show_all {
        entries.push(virtual_entry(path, ".")?);
        entries.push(virtual_entry(path, "..")?);
    }

    for item in fs::read_dir(path).map_err(|err| err.to_string())? {
        let item = item.map_err(|err| err.to_string())?;
        let name = item.file_name();
        if !should_show(&name, config) {
            continue;
        }

        let entry_path = item.path();
        let metadata = fs::symlink_metadata(&entry_path).map_err(|err| err.to_string())?;
        entries.push(EntryInfo {
            name,
            path: entry_path,
            metadata,
        });
    }

    Ok(entries)
}

fn virtual_entry(path: &Path, name: &str) -> Result<EntryInfo, String> {
    let entry_path = path.join(name);
    let metadata = fs::symlink_metadata(&entry_path).map_err(|err| err.to_string())?;

    Ok(EntryInfo {
        name: OsString::from(name),
        path: entry_path,
        metadata,
    })
}

fn should_show(name: &OsString, config: &Config) -> bool {
    let name = name.to_string_lossy();
    if config.show_all {
        true
    } else if config.almost_all {
        name != "." && name != ".."
    } else {
        !name.starts_with('.')
    }
}

fn sort_entries(entries: &mut [EntryInfo], config: &Config) {
    if matches!(config.sort_mode, SortMode::Unsorted) {
        return;
    }

    entries.sort_by(|left, right| {
        let ordering = match config.sort_mode {
            SortMode::Name => compare_name(left, right),
            SortMode::Time => compare_time(left, right),
            SortMode::Size => compare_size(left, right),
            SortMode::Unsorted => Ordering::Equal,
        };

        if config.reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

fn compare_name(left: &EntryInfo, right: &EntryInfo) -> Ordering {
    display_name_from_os(&left.name).cmp(&display_name_from_os(&right.name))
}

fn compare_time(left: &EntryInfo, right: &EntryInfo) -> Ordering {
    right
        .metadata
        .modified()
        .ok()
        .cmp(&left.metadata.modified().ok())
        .then_with(|| compare_name(left, right))
}

fn compare_size(left: &EntryInfo, right: &EntryInfo) -> Ordering {
    right
        .metadata
        .len()
        .cmp(&left.metadata.len())
        .then_with(|| compare_name(left, right))
}

fn print_section(
    header_path: Option<&Path>,
    entries: &[EntryInfo],
    config: &Config,
    show_header: bool,
    first_section: &mut bool,
) -> Result<(), String> {
    if show_header {
        if !*first_section {
            println!();
        }

        if let Some(path) = header_path {
            if config.count_children {
                println!("{}: [{} entries]", path.display(), entries.len());
            } else {
                println!("{}:", path.display());
            }
        }

        *first_section = false;
    }

    if entries.is_empty() {
        return Ok(());
    }

    if config.long {
        print_long(entries, config)?;
        return Ok(());
    }

    if config.one_per_line || !io::stdout().is_terminal() {
        for entry in entries {
            println!("{}", render_name(entry, config));
        }
        return Ok(());
    }

    print_columns(entries, config)?;
    Ok(())
}

fn print_long(entries: &[EntryInfo], config: &Config) -> Result<(), String> {
    let names: Vec<String> = entries
        .iter()
        .map(|entry| render_name(entry, config))
        .collect();
    let sizes: Vec<String> = entries
        .iter()
        .map(|entry| format_size(entry.metadata.len(), config.human_readable))
        .collect();
    let size_width = sizes.iter().map(|value| value.len()).max().unwrap_or(1);

    for ((entry, name), size) in entries.iter().zip(names.iter()).zip(sizes.iter()) {
        let perms = format_permissions(&entry.metadata);
        let mtime = format_system_time(entry.metadata.modified().ok());
        println!(
            "{} {:>width$} {} {}",
            perms,
            size,
            mtime,
            name,
            width = size_width
        );
    }

    Ok(())
}

fn print_columns(entries: &[EntryInfo], config: &Config) -> Result<(), String> {
    let names: Vec<String> = entries
        .iter()
        .map(|entry| render_name(entry, config))
        .collect();
    let max_width = names.iter().map(|name| name.len()).max().unwrap_or(0);
    let col_width = max_width + 2;
    let terminal_width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(80);
    let cols = usize::max(1, terminal_width / col_width.max(1));
    let rows = (names.len() + cols - 1) / cols;
    let mut stdout = io::stdout().lock();

    for row in 0..rows {
        for col in 0..cols {
            let index = col * rows + row;
            if let Some(name) = names.get(index) {
                if col + 1 == cols || index + rows >= names.len() {
                    write!(stdout, "{name}").map_err(|err| err.to_string())?;
                } else {
                    write!(stdout, "{name:<width$}", width = col_width)
                        .map_err(|err| err.to_string())?;
                }
            }
        }
        writeln!(stdout).map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn render_name(entry: &EntryInfo, config: &Config) -> String {
    let mut text = display_name_from_os(&entry.name);
    if config.classify {
        if let Some(suffix) = classify_suffix(&entry.metadata) {
            text.push(suffix);
        }
    }

    match color_mode(config) {
        ColorDecision::Never => text,
        ColorDecision::Always => colorize(&text, color_code(entry)),
        ColorDecision::Auto if io::stdout().is_terminal() => colorize(&text, color_code(entry)),
        ColorDecision::Auto => text,
    }
}

fn color_mode(config: &Config) -> ColorDecision {
    if env::var_os("NO_COLOR").is_some() {
        return ColorDecision::Never;
    }

    match config.color {
        ColorMode::Auto => ColorDecision::Auto,
        ColorMode::Always => ColorDecision::Always,
        ColorMode::Never => ColorDecision::Never,
    }
}

enum ColorDecision {
    Auto,
    Always,
    Never,
}

fn color_code(entry: &EntryInfo) -> &'static str {
    if is_directory_entry(entry) {
        "34"
    } else if is_symlink(&entry.metadata) {
        "36"
    } else if is_executable(&entry.metadata) {
        "32"
    } else {
        "0"
    }
}

fn colorize(text: &str, code: &str) -> String {
    if code == "0" {
        text.to_string()
    } else {
        format!("\x1b[{code}m{text}\x1b[0m")
    }
}

fn display_name_from_path(path: &Path) -> OsString {
    path.file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| path.as_os_str().to_os_string())
}

fn display_name_from_os(name: &OsString) -> String {
    name.to_string_lossy().into_owned()
}

fn is_dot_entry(name: &OsString) -> bool {
    matches!(name.to_str(), Some(".") | Some(".."))
}

fn is_directory_entry(entry: &EntryInfo) -> bool {
    entry.metadata.is_dir()
}

fn is_symlink(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn is_executable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &Metadata) -> bool {
    false
}

fn classify_suffix(metadata: &Metadata) -> Option<char> {
    if metadata.is_dir() {
        Some('/')
    } else if is_symlink(metadata) {
        Some('@')
    } else if is_executable(metadata) {
        Some('*')
    } else {
        None
    }
}

fn format_permissions(metadata: &Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = metadata.permissions().mode();
        let file_type = if metadata.is_dir() {
            'd'
        } else if metadata.file_type().is_symlink() {
            'l'
        } else {
            '-'
        };

        let mut out = String::with_capacity(10);
        out.push(file_type);
        for (read, write, exec) in [
            (0o400, 0o200, 0o100),
            (0o040, 0o020, 0o010),
            (0o004, 0o002, 0o001),
        ] {
            out.push(if mode & read != 0 { 'r' } else { '-' });
            out.push(if mode & write != 0 { 'w' } else { '-' });
            out.push(if mode & exec != 0 { 'x' } else { '-' });
        }
        out
    }

    #[cfg(not(unix))]
    {
        let file_type = if metadata.is_dir() { 'd' } else { '-' };
        let readonly = if metadata.permissions().readonly() {
            'r'
        } else {
            'w'
        };
        format!("{file_type}{readonly}--------")
    }
}

fn format_size(bytes: u64, human_readable: bool) -> String {
    if !human_readable || bytes < 1024 {
        return bytes.to_string();
    }

    const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
    let mut value = bytes as f64;
    let mut unit_index = 0usize;

    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        return bytes.to_string();
    }

    if value >= 10.0 {
        format!("{value:.0}{}", UNITS[unit_index])
    } else {
        format!("{value:.1}{}", UNITS[unit_index])
    }
}

fn format_system_time(time: Option<SystemTime>) -> String {
    match time {
        Some(time) => format_local_or_utc(time).unwrap_or_else(|| String::from("-")),
        None => String::from("-"),
    }
}

fn format_local_or_utc(time: SystemTime) -> Option<String> {
    if let Some(tm) = local_time(time) {
        return Some(format_tm(&tm, false));
    }

    utc_time(time).map(|tm| format_tm(&tm, true))
}

fn format_tm(tm: &NativeTm, utc_suffix: bool) -> String {
    if utc_suffix {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    } else {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

fn local_time(time: SystemTime) -> Option<NativeTm> {
    let platform_time = system_time_to_platform_time(time)?;
    let mut tm = NativeTm::default();

    #[cfg(unix)]
    unsafe {
        if localtime_r(
            &platform_time as *const PlatformTime,
            &mut tm as *mut NativeTm,
        )
        .is_null()
        {
            None
        } else {
            Some(tm)
        }
    }

    #[cfg(windows)]
    unsafe {
        if _localtime64_s(
            &mut tm as *mut NativeTm,
            &platform_time as *const PlatformTime,
        ) == 0
        {
            Some(tm)
        } else {
            None
        }
    }
}

fn utc_time(time: SystemTime) -> Option<NativeTm> {
    let platform_time = system_time_to_platform_time(time)?;
    let mut tm = NativeTm::default();

    #[cfg(unix)]
    unsafe {
        if gmtime_r(
            &platform_time as *const PlatformTime,
            &mut tm as *mut NativeTm,
        )
        .is_null()
        {
            None
        } else {
            Some(tm)
        }
    }

    #[cfg(windows)]
    unsafe {
        if _gmtime64_s(
            &mut tm as *mut NativeTm,
            &platform_time as *const PlatformTime,
        ) == 0
        {
            Some(tm)
        } else {
            None
        }
    }
}

fn system_time_to_platform_time(time: SystemTime) -> Option<PlatformTime> {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok()?,
        Err(err) => -i64::try_from(err.duration().as_secs()).ok()?,
    };

    #[cfg(unix)]
    {
        PlatformTime::try_from(seconds).ok()
    }

    #[cfg(windows)]
    {
        Some(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_and_paths() {
        let config = parse_args(vec![
            String::from("-al1RcH"),
            String::from("--sort=time"),
            String::from("src"),
            String::from("Cargo.toml"),
        ])
        .unwrap();

        assert!(config.show_all);
        assert!(config.long);
        assert!(config.one_per_line);
        assert!(config.recursive);
        assert!(config.count_children);
        assert!(config.human_readable);
        assert_eq!(config.sort_mode, SortMode::Time);
        assert_eq!(
            config.paths,
            vec![PathBuf::from("src"), PathBuf::from("Cargo.toml")]
        );
    }

    #[test]
    fn hidden_filtering_matches_expectations() {
        let mut config = Config::default();
        assert!(!should_show(&OsString::from(".gitignore"), &config));

        config.almost_all = true;
        assert!(should_show(&OsString::from(".gitignore"), &config));
        assert!(!should_show(&OsString::from("."), &config));

        config.show_all = true;
        assert!(should_show(&OsString::from("."), &config));
    }

    #[test]
    fn human_readable_sizes_are_compact() {
        assert_eq!(format_size(999, true), "999");
        assert_eq!(format_size(1_024, true), "1.0K");
        assert_eq!(format_size(1_572_864, true), "1.5M");
    }

    #[test]
    fn unix_timestamps_are_formatted_as_utc() {
        let tm = NativeTm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 1,
            tm_mon: 0,
            tm_year: 70,
            tm_wday: 4,
            tm_yday: 0,
            tm_isdst: 0,
            #[cfg(unix)]
            tm_gmtoff: 0,
            #[cfg(unix)]
            tm_zone: std::ptr::null(),
        };

        assert_eq!(format_tm(&tm, true), "1970-01-01 00:00:00 UTC");
    }
}
