# rustycat (`rcat`)

A colorized `cat`, written in Rust — inspired by [ccat](https://github.com/owenthereal/ccat).

Prints file contents (or stdin) to your terminal with syntax highlighting,
auto-detected from the file extension, using [syntect](https://github.com/trishume/syntect)
(the same highlighting engine that powers `bat` and Sublime Text).

<img width="1242" height="659" alt="Screenshot 2026-07-19 at 2 45 50 PM" src="https://github.com/user-attachments/assets/21417c9c-0871-4985-8af3-c45e2f75cb13" />


## Install

### Download a binary

Download the archive for your computer from the project's
[GitHub Releases](https://github.com/KwisatzJim/rustycat/releases) page. Builds
are provided for Linux (x86_64 and ARM64), macOS (Apple Silicon and Intel), and
Windows (x86_64). Each release includes a `SHA256SUMS` file for verifying the
downloads.

Extract the archive, then move `rcat` (`rcat.exe` on Windows) somewhere listed
in your system's `PATH`.

### Install from source

Rustycat requires Rust 1.85 or newer. Install Rust through
[rustup](https://rustup.rs/), then run:

```
cargo install --git https://github.com/KwisatzJim/rustycat
```

This installs the `rcat` command in Cargo's binary directory (normally
`~/.cargo/bin`).

## Build locally

```bash
git clone https://github.com/KwisatzJim/rustycat.git
cd rustycat
cargo build --release --locked
```

The binary is produced at `target/release/rcat` (`rcat.exe` on Windows).

## Usage

```
rcat [OPTIONS] [FILES]...
```

Examples:

```
rcat main.rs                  # highlight a single file
rcat -n main.rs                # ...with line numbers
rcat main.rs Cargo.toml        # multiple files, each with a ==> filename <== header
cat notes.txt | rcat first.txt - last.txt  # read stdin at `-` between files
cat main.rs | rcat -l rust     # highlight stdin, forcing the "rust" language
rcat -p file.txt                # plain mode, behaves like regular cat
rcat --paging=never large.log   # disable automatic paging
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
| `--color <WHEN>` | Choose `auto`, `always`, or `never` |
| `--paging <WHEN>` | Choose `auto`, `always`, or `never` (default: `auto`) |
| `--list-themes` | List available themes and exit |
| `--list-languages` | List supported languages and exit |

By default, color is automatically disabled when output is piped/redirected
(not a TTY) and re-enabled when writing to a real terminal — same convention
as tools like `ls --color=auto`. Automatic mode also disables color when the
`NO_COLOR` environment variable is set or `TERM=dumb`.

### Automatic paging

When output is going to an interactive terminal, Rustycat automatically sends
it through `less -RFX`. Short output exits the pager immediately, while output
larger than one screen remains scrollable. Piped or redirected output never
starts a pager automatically. Use `--paging=always` to force paging or
`--paging=never` to disable it. If `less` is unavailable, automatic mode safely
falls back to direct output.

### Integration with other tools

Disable paging when continuously monitoring a file. Specify the syntax when
stdin does not provide a filename; this example assumes one JSON object per
line:

```bash
tail -f application.jsonl | rcat --paging=never --color=always -l json
```

Combine Rustycat with `git show` to view an older version of a file with the
correct syntax highlighting:

```bash
git show v0.1.0:src/main.rs | rcat -l rs
```

Rustycat can also serve as `man`'s pager. The bundled syntax set does not have a
specialized man-page grammar, but this preserves the formatted page and forces
terminal color:

```bash
export MANPAGER='rcat --color=always -l "Plain Text"'
man 2 select
```

## Notes

The checked-in `Cargo.lock` pins exact dependency versions so local and release
builds use the same dependency set.
