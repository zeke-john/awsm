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

use app::{Action, App, Screen};
use event::{Event, EventHandler};
use keys::{Focus, Mode};

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

    if app.screen == Screen::Main {
        terminal.draw(|frame| ui::render(&app, frame))?;
        app.init_aws().await;
    }

    while app.running {
        terminal.draw(|frame| ui::render(&app, frame))?;

        match events.next().await {
            Some(Event::Key(key)) => {
                match app.screen {
                    Screen::ProfilePicker => {
                        let action = match (key.code, key.modifiers) {
                            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
                            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Action::ProfileDown,
                            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Action::ProfileUp,
                            (KeyCode::Enter, _) => Action::ProfileSelect,
                            _ => Action::None,
                        };
                        let needs_init = action == Action::ProfileSelect;
                        app.update(action);
                        if needs_init && app.screen == Screen::Main {
                            terminal.draw(|frame| ui::render(&app, frame))?;
                            app.init_aws().await;
                        }
                    }
                    Screen::Main => {
                        if app.show_help {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                                    app.update(Action::ToggleHelp);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        let action = match app.mode {
                            Mode::Normal => handle_normal_key(key.code, key.modifiers, &app),
                            Mode::Insert => Action::None,
                            Mode::Command => Action::None,
                        };
                        app.update(action);
                    }
                }
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

fn handle_normal_key(code: KeyCode, modifiers: KeyModifiers, app: &App) -> Action {
    match (code, modifiers) {
        (KeyCode::Char('q'), _) => Action::Quit,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Char('?'), _) => Action::ToggleHelp,
        (KeyCode::Tab, _) => Action::ToggleFocus,
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => match app.focus {
            Focus::Sidebar => Action::SidebarDown,
            Focus::Main => Action::None,
        },
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => match app.focus {
            Focus::Sidebar => Action::SidebarUp,
            Focus::Main => Action::None,
        },
        (KeyCode::Enter, _) => match app.focus {
            Focus::Sidebar => Action::SelectService,
            Focus::Main => Action::None,
        },
        (KeyCode::Esc, _) => match app.focus {
            Focus::Main => Action::SetFocus(Focus::Sidebar),
            Focus::Sidebar => Action::Quit,
        },
        _ => Action::None,
    }
}
