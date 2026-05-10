# ls-rs

`ls-rs` is a small `ls`-style directory lister written in Rust with no external dependencies.

## Features

- Lists files and directories
- Supports recursion with `-R`
- Supports `-a`, `-A`, `-l`, `-1`, `-r`, `-d`, `-F`, `-c`, `-H`, `-t`, `-S`, and `-U`
- Supports `--color=auto|always|never`
- Uses ANSI colors when output is a terminal
- Works on Linux, macOS, and Windows with the Rust standard library only

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run -- .
cargo run -- -la /tmp
cargo run -- -RH --sort=size .
cargo run -- --count-children .
```

## Notes

- Long listing mode prints local time in `YYYY-MM-DD HH:MM:SS` form, with UTC used only as a fallback if local conversion fails.
- Human-readable sizes use binary units such as `K`, `M`, and `G`.
- `--count-children` shows the number of visible direct entries in each directory section.
- The implementation remains dependency-free so it can build cleanly on different platforms.
