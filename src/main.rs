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
use std::process::ExitCode;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::as_24_bit_terminal_escaped;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
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
}

fn main() -> ExitCode {
    let args = Args::parse();

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
            io::stdout().is_terminal() && !no_color_requested() && !terminal_is_dumb()
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

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut had_error = false;
    let mut line_number = 0;

    if args.files.is_empty() {
        let stdin = io::stdin();
        let mut input = stdin.lock();

        if theme.is_none() {
            if let Err(e) = print_plain_content(&mut input, args.number, &mut line_number, &mut out)
            {
                eprintln!("rcat: failed to process stdin: {e}");
                return ExitCode::FAILURE;
            }
            if let Err(e) = out.flush() {
                eprintln!("rcat: failed to flush output: {e}");
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
        if let Err(e) = out.flush() {
            eprintln!("rcat: failed to flush output: {e}");
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

        let result = if let Some(theme) = theme {
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

    if let Err(e) = out.flush() {
        eprintln!("rcat: failed to flush output: {e}");
        return ExitCode::FAILURE;
    }
    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn terminal_is_dumb() -> bool {
    env::var_os("TERM").is_some_and(|value| value == "dumb")
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
