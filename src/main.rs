//! rustycat (rcat) — a colorized `cat`, inspired by ccat
//! https://github.com/owenthereal/ccat
//!
//! Reads one or more files (or stdin) and prints them to the terminal with
//! syntax highlighting, detected automatically from the file extension
//! (or forced with --language).

use clap::Parser;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::as_24_bit_terminal_escaped;

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
    #[arg(short = 'p', long)]
    plain: bool,

    /// Always colorize, even when output is not a terminal (e.g. piped)
    #[arg(short = 'f', long = "force-color")]
    force_color: bool,
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

    // Decide whether to colorize at all.
    let colorize = !args.plain && (args.force_color || io::stdout().is_terminal());

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

    if args.files.is_empty() {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            eprintln!("rcat: failed to read stdin: {e}");
            return ExitCode::FAILURE;
        }
        let syntax = resolve_syntax(&ss, None, args.language.as_deref());
        print_content(&buf, syntax, theme, &ss, args.number, &mut out);
        let _ = out.flush();
        return ExitCode::SUCCESS;
    }

    let multiple = args.files.len() > 1;
    for (i, path) in args.files.iter().enumerate() {
        let content = match fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    eprintln!("rcat: {}: binary file (skipping)", path.display());
                    had_error = true;
                    continue;
                }
            },
            Err(e) => {
                eprintln!("rcat: {}: {e}", path.display());
                had_error = true;
                continue;
            }
        };

        if multiple {
            if colorize {
                let _ = writeln!(out, "\x1b[1;32m==> {} <==\x1b[0m", path.display());
            } else {
                let _ = writeln!(out, "==> {} <==", path.display());
            }
        }

        let syntax = resolve_syntax(&ss, Some(path.as_path()), args.language.as_deref());
        print_content(&content, syntax, theme, &ss, args.number, &mut out);

        if multiple && i + 1 != args.files.len() {
            let _ = writeln!(out);
        }
    }

    let _ = out.flush();
    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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

    if let Some(p) = path {
        if let Ok(Some(syntax)) = ss.find_syntax_for_file(p) {
            return syntax;
        }
    }

    ss.find_syntax_plain_text()
}

/// Highlight (or plainly print) `content` line by line.
fn print_content(
    content: &str,
    syntax: &SyntaxReference,
    theme: Option<&Theme>,
    ss: &SyntaxSet,
    number_lines: bool,
    out: &mut impl Write,
) {
    match theme {
        None => {
            // Plain mode: behave like `cat`, optionally with -n numbering.
            for (i, line) in content.lines().enumerate() {
                if number_lines {
                    let _ = write!(out, "{:>6}\t", i + 1);
                }
                let _ = writeln!(out, "{line}");
            }
        }
        Some(theme) => {
            let mut h = HighlightLines::new(syntax, theme);
            for (i, line) in content.lines().enumerate() {
                let line_with_nl = format!("{line}\n");
                let ranges: Vec<(Style, &str)> =
                    h.highlight_line(&line_with_nl, ss).unwrap_or_default();

                if number_lines {
                    let _ = write!(out, "\x1b[38;5;244m{:>6}\x1b[0m\t", i + 1);
                }

                // The highlighted text already contains the line's trailing
                // newline (syntect needs it fed in for correct multi-line
                // highlighting state), so strip it before adding our own
                // newline below — otherwise every line prints with a blank
                // line after it.
                let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                let trimmed = escaped.trim_end_matches('\n');
                let _ = writeln!(out, "{trimmed}\x1b[0m");
            }
        }
    }
}
