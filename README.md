# rustycat (`rcat`)

A colorized `cat`, written in Rust — inspired by [ccat](https://github.com/owenthereal/ccat).

Prints file contents (or stdin) to your terminal with syntax highlighting,
auto-detected from the file extension, using [syntect](https://github.com/trishume/syntect)
(the same highlighting engine that powers `bat` and Sublime Text).

<img width="1242" height="659" alt="Screenshot 2026-07-19 at 2 45 50 PM" src="https://github.com/user-attachments/assets/21417c9c-0871-4985-8af3-c45e2f75cb13" />


## Build

```
cargo build --release
```

The binary is produced at `target/release/rcat`.

## Usage

```
rcat [OPTIONS] [FILES]...
```

Examples:

```
rcat main.rs                  # highlight a single file
rcat -n main.rs                # ...with line numbers
rcat main.rs Cargo.toml        # multiple files, each with a ==> filename <== header
cat main.rs | rcat -l rust     # highlight stdin, forcing the "rust" language
rcat -p file.txt                # plain mode, behaves like regular cat
rcat --list-themes              # show available color themes
rcat --list-languages           # show all supported languages/syntaxes
rcat -t "Solarized (dark)" main.rs   # pick a specific theme
```

### Options

| Flag | Description |
|---|---|
| `-n`, `--number` | Number all output lines |
| `-l`, `--language <LANG>` | Force a specific language instead of auto-detecting from extension |
| `-t`, `--theme <THEME>` | Color theme (default: `base16-ocean.dark`) |
| `-p`, `--plain` | Disable colorization entirely (plain `cat` behavior) |
| `-f`, `--force-color` | Colorize even when stdout isn't a terminal (e.g. when piping to `less -R`) |
| `--list-themes` | List available themes and exit |
| `--list-languages` | List supported languages and exit |

By default, color is automatically disabled when output is piped/redirected
(not a TTY) and re-enabled when writing to a real terminal — same convention
as tools like `ls --color=auto`.

## Notes

This build pins a few dependency versions (`syntect` 5.1, `clap` 4.4, etc.)
for compatibility with older toolchains. If you're building with a recent
stable Rust (1.80+), feel free to run `cargo update` to pick up newer
versions — they'll work too.
