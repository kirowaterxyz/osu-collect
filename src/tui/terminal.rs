use crate::app::runtime::InputEvent;
use crate::tui::bg;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, style::Color};
use std::{
    io::{self, Stdout, Write},
    sync::Once,
    thread,
    time::Duration,
};
use tokio::sync::mpsc;

pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

static PANIC_HOOK: Once = Once::new();

pub fn setup_terminal() -> io::Result<TuiTerminal> {
    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _ = set_terminal_bg(&mut stdout, bg());
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

pub fn cleanup_terminal(terminal: &mut TuiTerminal) -> io::Result<()> {
    disable_raw_mode()?;
    let backend = terminal.backend_mut();
    let _ = reset_terminal_bg(backend);
    execute!(backend, LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let mut stdout = io::stdout();
            let _ = reset_terminal_bg(&mut stdout);
            let _ = execute!(stdout, LeaveAlternateScreen);
            let _ = disable_raw_mode();
            default(info);
        }));
    });
}

pub fn set_terminal_bg<W: Write>(out: &mut W, color: Color) -> io::Result<()> {
    if let Color::Rgb(r, g, b) = color {
        write!(out, "\x1b]11;rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\")?;
        out.flush()?;
    }
    Ok(())
}

pub fn reset_terminal_bg<W: Write>(out: &mut W) -> io::Result<()> {
    write!(out, "\x1b]111\x1b\\")?;
    out.flush()
}

#[cfg(test)]
#[path = "../../tests/unit/tui_terminal.rs"]
mod tests;

pub fn spawn_input_thread(tx: mpsc::UnboundedSender<InputEvent>) -> Option<thread::JoinHandle<()>> {
    let tick_rate = Duration::from_millis(50);
    thread::Builder::new()
        .name("osu-collect-input".into())
        .spawn(move || {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                            if tx.send(InputEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Resize(_, _)) => {
                            if tx.send(InputEvent::Resize).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                } else if tx.send(InputEvent::Tick).is_err() {
                    break;
                }
            }
        })
        .ok()
}
