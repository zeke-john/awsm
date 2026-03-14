#![allow(dead_code, unused_imports)]

mod app;
mod aws;
mod error;
mod event;
mod keys;
mod ui;

use std::io;
use std::panic;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{Action, App};
use event::{Event, EventHandler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen, cursor::Show);
        prev_hook(info);
    }));

    terminal::enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let mut events = EventHandler::new(Duration::from_millis(250));

    while app.running {
        terminal.draw(|frame| ui::render(&app, frame))?;

        match events.next().await {
            Some(Event::Key(key)) => {
                let action = match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) => Action::Quit,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
                    (KeyCode::Esc, _) => Action::Quit,
                    _ => Action::None,
                };
                app.update(action);
            }
            Some(Event::Tick) => {}
            Some(Event::Resize(_, _)) => {}
            None => break,
        }
    }

    terminal::disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen, cursor::Show)?;

    Ok(())
}
