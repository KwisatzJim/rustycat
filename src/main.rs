//! rustycat (rcat) — a colorized `cat`, inspired by ccat
//! https://github.com/owenthereal/ccat
//!
//! Reads one or more files (or stdin) and prints them to the terminal with
//! syntax highlighting, detected automatically from the file extension
//! (or forced with --language).

use clap::{Parser, ValueEnum};
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitCode, Stdio};

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::as_24_bit_terminal_escaped;

mod viewer;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PagingChoice {
    Auto,
    Always,
    Never,
}

enum Output {
    Stdout(io::BufWriter<io::Stdout>),
    Pager {
        child: Child,
        stdin: Option<ChildStdin>,
    },
}

impl Output {
    fn start(
        choice: PagingChoice,
        automatic_paging_available: bool,
        markdown_preview: bool,
    ) -> io::Result<Self> {
        let should_page = match choice {
            PagingChoice::Auto => automatic_paging_available,
            PagingChoice::Always => true,
            PagingChoice::Never => false,
        };

        if should_page {
            let mut command = Command::new("less");
            command.args(["-R", "-F", "-X"]).stdin(Stdio::piped());
            if markdown_preview {
                // Rendered Markdown contains UTF-8 table borders and bullets.
                command.env("LESSCHARSET", "utf-8");
            }
            match command.spawn() {
                Ok(mut child) => {
                    let stdin = child
                        .stdin
                        .take()
                        .ok_or_else(|| io::Error::other("pager stdin was unavailable"))?;
                    return Ok(Self::Pager {
                        child,
                        stdin: Some(stdin),
                    });
                }
                Err(_) if matches!(choice, PagingChoice::Auto) => {}
                Err(error) => return Err(error),
            }
        }

        Ok(Self::Stdout(io::BufWriter::new(io::stdout())))
    }

    fn is_pager(&self) -> bool {
        matches!(self, Self::Pager { .. })
    }

    fn finish(mut self) -> io::Result<()> {
        self.flush()?;
        if let Self::Pager { child, stdin } = &mut self {
            stdin.take();
            let status = child.wait()?;
            if !status.success() {
                return Err(io::Error::other(format!(
                    "pager exited with status {status}"
                )));
            }
        }
        Ok(())
    }
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stdout(stdout) => stdout.write(buf),
            Self::Pager { stdin, .. } => write_to_pager(stdin, buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stdout(stdout) => stdout.flush(),
            Self::Pager { stdin, .. } => match stdin.as_mut().map(Write::flush) {
                Some(Err(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
                    stdin.take();
                    Ok(())
                }
                Some(result) => result,
                None => Ok(()),
            },
        }
    }
}

fn write_to_pager(stdin: &mut Option<ChildStdin>, buf: &[u8]) -> io::Result<usize> {
    let Some(writer) = stdin.as_mut() else {
        return Ok(buf.len());
    };
    match writer.write(buf) {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
            stdin.take();
            Ok(buf.len())
        }
        result => result,
    }
}

/// A colorized `cat`, written in Rust.
#[derive(Parser, Debug)]
#[command(name = "rcat", version, about, long_about = None)]
struct Args {
    /// Files to display. If omitted, reads from stdin.
    files: Vec<PathBuf>,

    /// Number all output lines
    #[arg(short = 'n', long)]
    number: bool,

    /// Render input as a Markdown preview
    #[arg(long, conflicts_with_all = ["plain", "number", "language"])]
    preview: bool,

    /// Open one Markdown file with Tab to toggle source and preview
    #[arg(long, conflicts_with_all = ["plain", "number", "language", "list_themes", "list_languages"])]
    interactive: bool,

    /// Force a specific language/syntax (e.g. "rust", "python", "yaml")
    #[arg(short = 'l', long)]
    language: Option<String>,

    /// Color theme to use
    #[arg(short = 't', long, default_value = "base16-ocean.dark")]
    theme: String,

    /// List available color themes and exit
    #[arg(long)]
    list_themes: bool,

    /// List supported languages and exit
    #[arg(long)]
    list_languages: bool,

    /// Disable colorized output, behave like plain `cat`
    #[arg(short = 'p', long, conflicts_with_all = ["force_color", "color"])]
    plain: bool,

    /// Always colorize, even when output is not a terminal (e.g. piped)
    #[arg(short = 'f', long = "force-color", conflicts_with = "color")]
    force_color: bool,

    /// When to use color: auto, always, or never
    #[arg(long, value_enum)]
    color: Option<ColorChoice>,

    /// When to use a pager: auto, always, or never
    #[arg(long, value_enum, default_value_t = PagingChoice::Auto)]
    paging: PagingChoice,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let interactive_terminal =
        io::stdin().is_terminal() && io::stdout().is_terminal() && paging_terminal_is_usable();
    if args.interactive || automatic_markdown_viewer(&args, interactive_terminal) {
        let result = (|| {
            if args.files.len() != 1
                || !args.files[0].extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
                })
            {
                return Err(io::Error::other("--interactive requires one Markdown file"));
            }
            if !interactive_terminal {
                return Err(io::Error::other(
                    "--interactive requires an interactive terminal",
                ));
            }
            let markdown = std::fs::read_to_string(&args.files[0])?;
            let color = args.force_color
                || match args.color.unwrap_or(ColorChoice::Auto) {
                    ColorChoice::Always => true,
                    ColorChoice::Never => false,
                    ColorChoice::Auto => !no_color_requested(),
                };
            viewer::show(&markdown, args.preview, color)
        })();
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("rcat: {error}");
                ExitCode::FAILURE
            }
        };
    }

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    if args.list_themes {
        let mut names: Vec<&String> = ts.themes.keys().collect();
        names.sort();
        for name in names {
            println!("{name}");
        }
        return ExitCode::SUCCESS;
    }

    if args.list_languages {
        let mut names: Vec<String> = ss.syntaxes().iter().map(|s| s.name.clone()).collect();
        names.sort();
        names.dedup();
        for name in names {
            println!("{name}");
        }
        return ExitCode::SUCCESS;
    }

    let stdout_is_terminal = io::stdout().is_terminal();
    let terminal_supports_paging = stdout_is_terminal && paging_terminal_is_usable();
    let mut out = match Output::start(args.paging, terminal_supports_paging, args.preview) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("rcat: failed to start pager: {error}");
            return ExitCode::FAILURE;
        }
    };

    let color_choice = if args.plain {
        ColorChoice::Never
    } else if args.force_color {
        ColorChoice::Always
    } else {
        args.color.unwrap_or(ColorChoice::Auto)
    };
    let colorize = match color_choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            (stdout_is_terminal || out.is_pager()) && !no_color_requested() && !terminal_is_dumb()
        }
    };

    let theme: Option<&Theme> = if colorize {
        match ts.themes.get(&args.theme) {
            Some(t) => Some(t),
            None => {
                eprintln!(
                    "rcat: unknown theme '{}', falling back to 'base16-ocean.dark'. Use --list-themes to see options.",
                    args.theme
                );
                ts.themes.get("base16-ocean.dark")
            }
        }
    } else {
        None
    };

    let mut had_error = false;
    let mut line_number = 0;

    if args.files.is_empty() {
        let stdin = io::stdin();
        let mut input = stdin.lock();

        if args.preview {
            if let Err(e) =
                print_markdown_preview(&mut input, colorize, &mut out).and_then(|()| out.finish())
            {
                eprintln!("rcat: failed to preview stdin: {e}");
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }

        if theme.is_none() {
            if let Err(e) = print_plain_content(&mut input, args.number, &mut line_number, &mut out)
            {
                eprintln!("rcat: failed to process stdin: {e}");
                return ExitCode::FAILURE;
            }
            if let Err(e) = out.finish() {
                eprintln!("rcat: failed to finish output: {e}");
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }

        let syntax = resolve_syntax(&ss, None, args.language.as_deref());
        if let Err(e) = print_highlighted_content(
            &mut input,
            syntax,
            theme.expect("colorized output has a theme"),
            &ss,
            args.number,
            &mut line_number,
            &mut out,
        ) {
            eprintln!("rcat: failed to process stdin: {e}");
            return ExitCode::FAILURE;
        }
        if let Err(e) = out.finish() {
            eprintln!("rcat: failed to finish output: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    let multiple = args.files.len() > 1;
    for (i, path) in args.files.iter().enumerate() {
        let stdin = io::stdin();
        let is_stdin = path == Path::new("-");
        let mut input: Box<dyn BufRead> = if is_stdin {
            Box::new(stdin.lock())
        } else {
            match File::open(path) {
                Ok(file) => Box::new(BufReader::new(file)),
                Err(e) => {
                    eprintln!("rcat: {}: {e}", path.display());
                    had_error = true;
                    continue;
                }
            }
        };

        if multiple {
            let label = if is_stdin {
                "standard input".to_string()
            } else {
                path.display().to_string()
            };
            let result = if colorize {
                writeln!(out, "\x1b[1;32m==> {label} <==\x1b[0m")
            } else {
                writeln!(out, "==> {label} <==")
            };
            if let Err(e) = result {
                eprintln!("rcat: failed to write output: {e}");
                return ExitCode::FAILURE;
            }
        }

        let result = if args.preview {
            print_markdown_preview(&mut input, colorize, &mut out)
        } else if let Some(theme) = theme {
            let syntax_path = (!is_stdin).then_some(path.as_path());
            let syntax = resolve_syntax(&ss, syntax_path, args.language.as_deref());
            print_highlighted_content(
                &mut input,
                syntax,
                theme,
                &ss,
                args.number,
                &mut line_number,
                &mut out,
            )
        } else {
            print_plain_content(&mut input, args.number, &mut line_number, &mut out)
        };
        if let Err(e) = result {
            eprintln!("rcat: failed to write output: {e}");
            return ExitCode::FAILURE;
        }

        let separator_result = if multiple && i + 1 != args.files.len() {
            writeln!(out)
        } else {
            Ok(())
        };
        if let Err(e) = separator_result {
            eprintln!("rcat: failed to write output: {e}");
            return ExitCode::FAILURE;
        }
    }

    if let Err(e) = out.finish() {
        eprintln!("rcat: failed to finish output: {e}");
        return ExitCode::FAILURE;
    }
    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn automatic_markdown_viewer(args: &Args, interactive_terminal: bool) -> bool {
    interactive_terminal
        && !matches!(args.paging, PagingChoice::Never)
        && !args.plain
        && !args.number
        && args.language.is_none()
        && !args.list_themes
        && !args.list_languages
        && args.files.len() == 1
        && args.files[0].extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
        })
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

/// Preview explicitly requested Markdown using the existing output/pager path.
fn print_markdown_preview(
    input: &mut impl BufRead,
    colorize: bool,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut markdown = String::new();
    input.read_to_string(&mut markdown)?;
    let skin = if colorize {
        termimad::MadSkin::default()
    } else {
        termimad::MadSkin::no_style()
    };
    let width = termimad::terminal_size().0 as usize;
    write!(out, "{}", skin.text(&markdown, Some(width.max(20))))
}

fn terminal_is_dumb() -> bool {
    env::var_os("TERM").is_some_and(|value| value == "dumb")
}

fn paging_terminal_is_usable() -> bool {
    env::var_os("TERM").is_some_and(|value| !value.is_empty() && value != "dumb")
}

/// Print bytes without changing their contents, optionally adding line numbers.
fn print_plain_content(
    input: &mut impl BufRead,
    number_lines: bool,
    line_number: &mut usize,
    out: &mut impl Write,
) -> io::Result<()> {
    if !number_lines {
        io::copy(input, out)?;
        return Ok(());
    }

    let mut line = Vec::new();
    while input.read_until(b'\n', &mut line)? != 0 {
        *line_number += 1;
        write!(out, "{:>6}\t", *line_number)?;
        out.write_all(&line)?;
        line.clear();
    }
    Ok(())
}

/// Pick a syntax definition: forced language > file extension/name > plain text.
fn resolve_syntax<'a>(
    ss: &'a SyntaxSet,
    path: Option<&Path>,
    forced_language: Option<&str>,
) -> &'a SyntaxReference {
    if let Some(lang) = forced_language {
        if let Some(syntax) = ss
            .find_syntax_by_token(lang)
            .or_else(|| ss.find_syntax_by_name(lang))
        {
            return syntax;
        }
        eprintln!("rcat: unknown language '{lang}', falling back to auto-detection");
    }

    if let Some(p) = path
        && let Ok(Some(syntax)) = ss.find_syntax_for_file(p)
    {
        return syntax;
    }

    ss.find_syntax_plain_text()
}

/// Read and highlight one line at a time so large inputs use bounded memory.
fn print_highlighted_content(
    input: &mut impl BufRead,
    syntax: &SyntaxReference,
    theme: &Theme,
    ss: &SyntaxSet,
    number_lines: bool,
    line_number: &mut usize,
    out: &mut impl Write,
) -> io::Result<()> {
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut bytes = Vec::new();

    while input.read_until(b'\n', &mut bytes)? != 0 {
        let line = std::str::from_utf8(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line_with_nl = format!("{line}\n");
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(&line_with_nl, ss)
            .unwrap_or_default();

        if number_lines {
            *line_number += 1;
            write!(out, "\x1b[38;5;244m{:>6}\x1b[0m\t", *line_number)?;
        }

        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        let trimmed = escaped.trim_end_matches('\n');
        writeln!(out, "{trimmed}\x1b[0m")?;
        bytes.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_viewer_respects_terminal_and_output_options() {
        for extension in ["md", "MD", "markdown"] {
            let args = Args::parse_from(["rcat", &format!("file.{extension}")]);
            assert!(automatic_markdown_viewer(&args, true));
            assert!(!automatic_markdown_viewer(&args, false));
        }
        for argv in [
            vec!["rcat", "--paging=never", "file.md"],
            vec!["rcat", "--plain", "file.md"],
            vec!["rcat", "--number", "file.md"],
            vec!["rcat", "--language=rust", "file.md"],
            vec!["rcat", "--list-themes", "file.md"],
            vec!["rcat", "--list-languages", "file.md"],
            vec!["rcat", "file.md", "other.md"],
            vec!["rcat", "file.rs"],
            vec!["rcat", "-"],
            vec!["rcat"],
        ] {
            assert!(!automatic_markdown_viewer(&Args::parse_from(argv), true));
        }
    }
}
