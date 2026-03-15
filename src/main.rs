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
use keys::{Focus, Mode, Service};
use ui::services::ServiceComponent;

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
        terminal.draw(|frame| ui::render(&mut app, frame))?;
        app.init_aws().await;
        load_service_data(&mut app).await;
    }

    while app.running {
        terminal.draw(|frame| ui::render(&mut app, frame))?;

        match events.next().await {
            Some(Event::Key(key)) => {
                match app.screen {
                    Screen::ProfilePicker => {
                        let action = match (key.code, key.modifiers) {
                            (KeyCode::Char('q'), _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
                            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Action::ProfileDown,
                            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Action::ProfileUp,
                            (KeyCode::Enter, _) => Action::ProfileSelect,
                            _ => Action::None,
                        };
                        let needs_init = action == Action::ProfileSelect;
                        app.update(action);
                        if needs_init && app.screen == Screen::Main {
                            terminal.draw(|frame| ui::render(&mut app, frame))?;
                            app.init_aws().await;
                            load_service_data(&mut app).await;
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

                        // Global keys always available
                        let global = match (key.code, key.modifiers) {
                            (KeyCode::Char('q'), _) => Some(Action::Quit),
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
                            (KeyCode::Char('?'), _) => Some(Action::ToggleHelp),
                            (KeyCode::Tab, _) => Some(Action::ToggleFocus),
                            (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Action::ToggleSidebar),
                            _ => None,
                        };

                        if let Some(action) = global {
                            let prev_service = app.active_service;
                            app.update(action);
                            if app.active_service != prev_service {
                                load_service_data(&mut app).await;
                            }
                            continue;
                        }

                        if app.focus == Focus::Main {
                            let service_action = match app.active_service {
                                Service::S3 => app.s3_view.handle_key(key),
                                _ => None,
                            };

                            if let Some(action) = service_action {
                                for queued in events.drain_keys() {
                                    if let Event::Key(qk) = queued {
                                        match app.active_service {
                                            Service::S3 => { app.s3_view.handle_key(qk); }
                                            _ => {}
                                        }
                                    }
                                }

                                match action {
                                    Action::ServiceEnter => {
                                        handle_s3_enter(&mut app).await;
                                    }
                                    Action::ServiceBack => {
                                        handle_s3_back(&mut app).await;
                                    }
                                    Action::S3Download => {
                                        handle_s3_download(&mut app).await;
                                    }
                                    _ => {
                                        app.update(action);
                                    }
                                }
                                continue;
                            }
                        }

                        let action = match app.mode {
                            Mode::Normal => handle_normal_key(key.code, key.modifiers, &app),
                            Mode::Insert => Action::None,
                            Mode::Command => Action::None,
                        };

                        let prev_service = app.active_service;
                        app.update(action);

                        if app.active_service != prev_service {
                            load_service_data(&mut app).await;
                        }
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

async fn load_service_data(app: &mut App) {
    match app.active_service {
        Service::S3 => {
            if app.s3_view.needs_bucket_load() {
                if let Some(ref aws) = app.aws {
                    match aws::s3::list_buckets(&aws.s3).await {
                        Ok(buckets) => app.s3_view.set_buckets(buckets),
                        Err(e) => app.s3_view.set_error(e),
                    }
                }
            }
        }
        _ => {}
    }
}

async fn handle_s3_enter(app: &mut App) {
    if app.s3_view.needs_bucket_load() {
        return;
    }

    match app.s3_view.screen_type() {
        "buckets" => {
            if let Some(bucket) = app.s3_view.selected_bucket().cloned() {
                app.s3_view.enter_bucket(bucket.name.clone());
                if let Some(ref aws) = app.aws {
                    match aws::s3::list_objects(&aws.s3, &bucket.name, "").await {
                        Ok(objects) => app.s3_view.set_objects(objects),
                        Err(e) => app.s3_view.set_error(e),
                    }
                }
            }
        }
        "objects" => {
            if let Some(obj) = app.s3_view.selected_object().cloned() {
                if obj.is_prefix {
                    let bucket = app.s3_view.current_bucket().to_string();
                    app.s3_view.enter_prefix(obj.key.clone());
                    if let Some(ref aws) = app.aws {
                        match aws::s3::list_objects(&aws.s3, &bucket, &obj.key).await {
                            Ok(objects) => app.s3_view.set_objects(objects),
                            Err(e) => app.s3_view.set_error(e),
                        }
                    }
                } else {
                    let bucket = app.s3_view.current_bucket().to_string();
                    app.s3_view.enter_detail();
                    if let Some(ref aws) = app.aws {
                        match aws::s3::get_object_detail(&aws.s3, &bucket, &obj.key).await {
                            Ok(detail) => app.s3_view.set_detail(detail),
                            Err(e) => app.s3_view.set_error(e),
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

async fn handle_s3_back(app: &mut App) {
    if app.s3_view.needs_object_load() {
        let bucket = app.s3_view.current_bucket().to_string();
        let prefix = app.s3_view.current_prefix().to_string();
        if let Some(ref aws) = app.aws {
            match aws::s3::list_objects(&aws.s3, &bucket, &prefix).await {
                Ok(objects) => app.s3_view.set_objects(objects),
                Err(e) => app.s3_view.set_error(e),
            }
        }
    }
}

async fn handle_s3_download(app: &mut App) {
    if let Some(ref detail) = app.s3_view.detail {
        let filename = detail.key.rsplit('/').next().unwrap_or(&detail.key);
        let downloads_dir = dirs_home()
            .map(|h| h.join("Downloads"))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let dest = downloads_dir.join(filename);

        let bucket = app.s3_view.current_bucket().to_string();
        let key = detail.key.clone();

        if let Some(ref aws) = app.aws {
            match aws::s3::download_object(&aws.s3, &bucket, &key, &dest).await {
                Ok(()) => {
                    app.s3_view.status_msg =
                        Some(format!("Downloaded to {}", dest.display()));
                }
                Err(e) => {
                    app.s3_view.status_msg = Some(format!("Error: {}", e));
                }
            }
        }
    }
}

fn handle_s3_copy_uri(app: &mut App) {
    if let Some(ref detail) = app.s3_view.detail {
        let uri = aws::s3::s3_uri(app.s3_view.current_bucket(), &detail.key);
        copy_to_clipboard(&uri);
        app.s3_view.status_msg = Some(format!("Copied: {}", uri));
    }
}

fn handle_s3_copy_arn(app: &mut App) {
    if let Some(ref detail) = app.s3_view.detail {
        let arn = format!(
            "arn:aws:s3:::{}/{}",
            app.s3_view.current_bucket(),
            detail.key
        );
        copy_to_clipboard(&arn);
        app.s3_view.status_msg = Some(format!("Copied: {}", arn));
    }
}

fn copy_to_clipboard(text: &str) {
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(std::path::PathBuf::from)
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
        (KeyCode::Esc, _) => Action::SetFocus(Focus::Sidebar),
        _ => Action::None,
    }
}
