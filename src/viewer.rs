use std::io::{self, Write};
use termimad::crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Print, ResetColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use termimad::{FmtText, MadSkin};

// Always restore the terminal, including when drawing or reading input fails.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub fn show(markdown: &str, mut preview: bool, color: bool) -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, Hide)?;
    let skin = if color {
        MadSkin::default()
    } else {
        MadSkin::no_style()
    };
    let source_skin = MadSkin::no_style();
    let mut offsets = [0usize; 2];
    loop {
        let (width, height) = terminal::size()?;
        let rows = height.saturating_sub(1) as usize;
        let text = if preview {
            skin.text(markdown, Some(width.max(1) as usize))
        } else {
            FmtText::raw_str(&source_skin, markdown, Some(width.max(1) as usize))
        };
        let offset = &mut offsets[usize::from(preview)];
        let max_offset = text.lines.len().saturating_sub(rows.max(1));
        *offset = (*offset).min(max_offset);
        queue!(out, ResetColor, Clear(ClearType::All))?;
        let rendered = text.to_string();
        for (row, line) in rendered.lines().skip(*offset).take(rows).enumerate() {
            queue!(out, MoveTo(0, row as u16), Print(line), ResetColor)?;
        }
        let mode = if preview { "Preview" } else { "Source" };
        let footer = format!("{mode} | Tab: toggle | arrows/PgUp/PgDn: scroll | q: quit");
        queue!(
            out,
            MoveTo(0, height.saturating_sub(1)),
            Print(footer.chars().take(width as usize).collect::<String>())
        )?;
        out.flush()?;
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(());
                }
                KeyCode::Tab | KeyCode::BackTab => preview = !preview,
                KeyCode::Down | KeyCode::Char('j') => {
                    *offset = offset.saturating_add(1).min(max_offset)
                }
                KeyCode::Up | KeyCode::Char('k') => *offset = offset.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    *offset = offset.saturating_add(rows.max(1)).min(max_offset)
                }
                KeyCode::PageUp => *offset = offset.saturating_sub(rows.max(1)),
                KeyCode::Home => *offset = 0,
                KeyCode::End => *offset = max_offset,
                _ => {}
            },
            _ => {}
        }
    }
}
