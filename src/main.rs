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
                        // Command mode: intercept all input
                        if app.mode == Mode::Command {
                            match key.code {
                                KeyCode::Esc => {
                                    app.mode = Mode::Normal;
                                    app.command_input.clear();
                                }
                                KeyCode::Enter => {
                                    let cmd = app.command_input.trim().to_string();
                                    app.mode = Mode::Normal;
                                    app.command_input.clear();
                                    if cmd == "q" || cmd == "quit" {
                                        app.quit();
                                    } else if let Some(region) = cmd.strip_prefix("region ") {
                                        let region = region.trim().to_string();
                                        if !region.is_empty() {
                                            app.region = region;
                                            app.aws = None;
                                            terminal.draw(|frame| ui::render(&mut app, frame))?;
                                            app.init_aws().await;
                                            // Reset all views
                                            reset_all_views(&mut app);
                                            load_service_data(&mut app).await;
                                        }
                                    } else if let Some(profile) = cmd.strip_prefix("profile ") {
                                        let profile = profile.trim().to_string();
                                        if !profile.is_empty() {
                                            app.profile = profile;
                                            app.aws = None;
                                            terminal.draw(|frame| ui::render(&mut app, frame))?;
                                            app.init_aws().await;
                                            reset_all_views(&mut app);
                                            load_service_data(&mut app).await;
                                        }
                                    }
                                }
                                KeyCode::Backspace => {
                                    if app.command_input.is_empty() {
                                        app.mode = Mode::Normal;
                                    } else {
                                        app.command_input.pop();
                                    }
                                }
                                KeyCode::Char(c) => {
                                    app.command_input.push(c);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        if app.show_help {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('?') => {
                                    app.update(Action::ToggleHelp);
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    app.help_scroll = app.help_scroll.saturating_add(1);
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    app.help_scroll = app.help_scroll.saturating_sub(1);
                                }
                                _ => {}
                            }
                            continue;
                        }

                        // Skip global keys when a service overlay is open
                        let overlay_open = match app.active_service {
                            Service::DynamoDB => app.dynamodb_view.has_overlay(),
                            Service::CloudWatch => app.cloudwatch_view.has_overlay(),
                            _ => false,
                        };

                        if !overlay_open {
                            // Global keys always available
                            let global = match (key.code, key.modifiers) {
                                (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
                                (KeyCode::Char('?'), _) => Some(Action::ToggleHelp),
                                (KeyCode::Tab, _) => Some(Action::ToggleFocus),
                                (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Action::ToggleSidebar),
                                (KeyCode::Char(':'), _) => Some(Action::SetMode(Mode::Command)),
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
                        }

                        if app.focus == Focus::Main {
                            let service_action = match app.active_service {
                                Service::S3 => app.s3_view.handle_key(key),
                                Service::DynamoDB => app.dynamodb_view.handle_key(key),
                                Service::Lambda => app.lambda_view.handle_key(key),
                                Service::SecretsManager => app.secrets_view.handle_key(key),
                                Service::CloudWatch => app.cloudwatch_view.handle_key(key),
                            };

                            if let Some(action) = service_action {
                                for queued in events.drain_keys() {
                                    if let Event::Key(qk) = queued {
                                        match app.active_service {
                                            Service::S3 => { app.s3_view.handle_key(qk); }
                                            Service::DynamoDB => { app.dynamodb_view.handle_key(qk); }
                                            Service::Lambda => { app.lambda_view.handle_key(qk); }
                                            Service::SecretsManager => { app.secrets_view.handle_key(qk); }
                                            Service::CloudWatch => { app.cloudwatch_view.handle_key(qk); }
                                        }
                                    }
                                }

                                match (&app.active_service, &action) {
                                    (Service::S3, Action::ServiceEnter) => {
                                        handle_s3_enter(&mut app).await;
                                    }
                                    (Service::S3, Action::ServiceBack) => {
                                        handle_s3_back(&mut app).await;
                                    }
                                    (Service::S3, Action::S3Download) => {
                                        handle_s3_download(&mut app).await;
                                    }
                                    (Service::DynamoDB, Action::ServiceEnter) => {
                                        handle_ddb_enter(&mut app).await;
                                    }
                                    (Service::DynamoDB, Action::DdbNextPage) => {
                                        handle_ddb_next_page(&mut app).await;
                                    }
                                    (Service::DynamoDB, Action::DdbSwitchIndex) => {
                                        handle_ddb_switch_index(&mut app).await;
                                    }
                                    (Service::DynamoDB, Action::DdbRunQuery) => {
                                        handle_ddb_run_query(&mut app).await;
                                    }
                                    (Service::DynamoDB, Action::ServiceBack) => {
                                        handle_ddb_back(&mut app).await;
                                    }
                                    (Service::Lambda, Action::ServiceEnter) => {
                                        handle_lambda_enter(&mut app).await;
                                    }
                                    (Service::Lambda, Action::ServiceBack) => {
                                    }
                                    (Service::CloudWatch, Action::ServiceEnter) => {
                                        handle_cw_enter(&mut app).await;
                                    }
                                    (Service::CloudWatch, Action::ServiceBack) => {
                                    }
                                    (Service::CloudWatch, Action::CwLoadEvents) => {
                                        handle_cw_load_events(&mut app).await;
                                    }
                                    (Service::CloudWatch, Action::CwNextPage) => {
                                        handle_cw_next_page(&mut app).await;
                                    }
                                    (Service::CloudWatch, Action::CwRefresh) => {
                                        handle_cw_refresh(&mut app).await;
                                    }
                                    (Service::CloudWatch, Action::CwRunSearch) => {
                                        handle_cw_search(&mut app).await;
                                    }
                                    (Service::CloudWatch, Action::CwRunInsights) => {
                                        handle_cw_insights(&mut app, &mut terminal).await;
                                    }
                                    (Service::CloudWatch, Action::CwSearchNextPage) => {
                                        handle_cw_search_next_page(&mut app).await;
                                    }
                                    (Service::SecretsManager, Action::ServiceEnter) => {
                                        handle_secrets_enter(&mut app).await;
                                    }
                                    (Service::SecretsManager, Action::ServiceBack) => {
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
            Some(Event::Tick) => {
                // Continue background search if still scanning empty pages
                if app.active_service == Service::CloudWatch
                    && app.cloudwatch_view.search_continuing
                {
                    let group = app.cloudwatch_view.active_group.clone();
                    let pattern = app.cloudwatch_view.search_pattern.clone();
                    let (start_ms, end_ms) = app.cloudwatch_view.search_time_millis();
                    let token = app.cloudwatch_view.next_token.clone();
                    cw_search_burst(&mut app, &group, &pattern, start_ms, end_ms, token).await;
                }
            }
            Some(Event::Resize(_, _)) => {}
            None => break,
        }
    }

    terminal::disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen, cursor::Show)?;

    Ok(())
}

fn reset_all_views(app: &mut App) {
    app.s3_view = Default::default();
    app.dynamodb_view = Default::default();
    app.lambda_view = Default::default();
    app.cloudwatch_view = Default::default();
    app.secrets_view = Default::default();
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
        Service::DynamoDB => {
            if app.dynamodb_view.needs_table_load() {
                if let Some(ref aws) = app.aws {
                    match aws::dynamodb::list_tables(&aws.dynamodb).await {
                        Ok(tables) => app.dynamodb_view.set_tables(tables),
                        Err(e) => app.dynamodb_view.set_error(e),
                    }
                }
            }
        }
        Service::Lambda => {
            if app.lambda_view.needs_function_load() {
                if let Some(ref aws) = app.aws {
                    match aws::lambda::list_functions(&aws.lambda).await {
                        Ok(functions) => app.lambda_view.set_functions(functions),
                        Err(e) => app.lambda_view.set_error(e),
                    }
                }
            }
        }
        Service::CloudWatch => {
            if app.cloudwatch_view.needs_group_load() {
                if let Some(ref aws) = app.aws {
                    match aws::cloudwatch::list_log_groups(&aws.cloudwatch).await {
                        Ok(groups) => app.cloudwatch_view.set_groups(groups),
                        Err(e) => app.cloudwatch_view.set_error(e),
                    }
                }
            }
        }
        Service::SecretsManager => {
            if app.secrets_view.needs_secret_load() {
                if let Some(ref aws) = app.aws {
                    match aws::secrets::list_secrets(&aws.secrets).await {
                        Ok(secrets) => app.secrets_view.set_secrets(secrets),
                        Err(e) => app.secrets_view.set_error(e),
                    }
                }
            }
        }
    }
}

async fn handle_cw_enter(app: &mut App) {
    match app.cloudwatch_view.screen_type() {
        "groups" => {
            if let Some(group) = app.cloudwatch_view.selected_group().cloned() {
                app.cloudwatch_view.enter_group(group.name.clone());
                if let Some(ref aws) = app.aws {
                    match aws::cloudwatch::list_log_streams(&aws.cloudwatch, &group.name).await {
                        Ok(streams) => app.cloudwatch_view.set_streams(streams),
                        Err(e) => app.cloudwatch_view.set_error(e),
                    }
                }
            }
        }
        "streams" => {
            if let Some(stream) = app.cloudwatch_view.selected_stream().cloned() {
                let group = app.cloudwatch_view.active_group.clone();
                app.cloudwatch_view.enter_stream(stream.name.clone());
                if let Some(ref aws) = app.aws {
                    match aws::cloudwatch::get_stream_events(
                        &aws.cloudwatch,
                        &group,
                        &stream.name,
                        None,
                    )
                    .await
                    {
                        Ok(result) => app.cloudwatch_view.set_events(result.events, result.next_token),
                        Err(e) => app.cloudwatch_view.set_error(e),
                    }
                }
            }
        }
        _ => {}
    }
}

async fn handle_cw_refresh(app: &mut App) {
    match app.cloudwatch_view.screen_type() {
        "groups" => {
            if let Some(ref aws) = app.aws {
                match aws::cloudwatch::list_log_groups(&aws.cloudwatch).await {
                    Ok(groups) => {
                        app.cloudwatch_view.set_groups(groups);
                        app.cloudwatch_view.refresh_flash = 6;
                    }
                    Err(e) => app.cloudwatch_view.set_error(e),
                }
            }
        }
        "streams" => {
            let group = app.cloudwatch_view.active_group.clone();
            if let Some(ref aws) = app.aws {
                match aws::cloudwatch::list_log_streams(&aws.cloudwatch, &group).await {
                    Ok(streams) => {
                        app.cloudwatch_view.set_streams(streams);
                        app.cloudwatch_view.refresh_flash = 6;
                    }
                    Err(e) => app.cloudwatch_view.set_error(e),
                }
            }
        }
        "events" => {
            let group = app.cloudwatch_view.active_group.clone();
            let stream = app.cloudwatch_view.active_stream.clone();
            if let Some(ref aws) = app.aws {
                match aws::cloudwatch::get_stream_events(
                    &aws.cloudwatch,
                    &group,
                    &stream,
                    None,
                )
                .await
                {
                    Ok(result) => {
                        app.cloudwatch_view.set_events(result.events, result.next_token);
                        app.cloudwatch_view.refresh_flash = 6;
                    }
                    Err(e) => app.cloudwatch_view.set_error(e),
                }
            }
        }
        _ => {}
    }
}

async fn handle_cw_load_events(app: &mut App) {
    let group = app.cloudwatch_view.active_group.clone();
    let stream = app.cloudwatch_view.active_stream.clone();

    if let Some(ref aws) = app.aws {
        match aws::cloudwatch::get_stream_events(&aws.cloudwatch, &group, &stream, None).await {
            Ok(result) => app.cloudwatch_view.set_events(result.events, result.next_token),
            Err(e) => app.cloudwatch_view.set_error(e),
        }
    }
}

async fn handle_cw_next_page(app: &mut App) {
    let group = app.cloudwatch_view.active_group.clone();
    let stream = app.cloudwatch_view.active_stream.clone();
    let token = app.cloudwatch_view.next_token.clone();

    if let (Some(aws), Some(next_token)) = (&app.aws, &token) {
        match aws::cloudwatch::get_stream_events(
            &aws.cloudwatch,
            &group,
            &stream,
            Some(&next_token),
        )
        .await
        {
            Ok(result) => app.cloudwatch_view.append_events(result.events, result.next_token),
            Err(e) => app.cloudwatch_view.set_error(e),
        }
    }
}

async fn handle_cw_search(app: &mut App) {
    let group = app.cloudwatch_view.active_group.clone();
    let pattern = app.cloudwatch_view.search_pattern.clone();
    let (start_ms, end_ms) = app.cloudwatch_view.search_time_millis();

    // Close popup, switch to events view in loading state
    app.cloudwatch_view.start_search_view();

    // Fetch a single burst of pages (skip empties, cap at 5 API calls)
    // then return control to the event loop so the UI stays responsive.
    // Auto-load will trigger CwSearchNextPage as the user scrolls.
    cw_search_burst(app, &group, &pattern, start_ms, end_ms, None).await;
}

async fn handle_cw_search_next_page(app: &mut App) {
    let group = app.cloudwatch_view.active_group.clone();
    let pattern = app.cloudwatch_view.search_pattern.clone();
    let (start_ms, end_ms) = app.cloudwatch_view.search_time_millis();

    let token = match app.cloudwatch_view.next_token.clone() {
        Some(t) => t,
        None => {
            app.cloudwatch_view.loading = false;
            return;
        }
    };

    cw_search_burst(app, &group, &pattern, start_ms, end_ms, Some(token)).await;
}

/// Fetch a small burst of filter_log_events pages (skipping empty ones),
/// then return so the event loop can process input. This keeps the UI responsive.
/// Sets search_continuing=true if more pages remain, so tick events drive the next burst.
async fn cw_search_burst(
    app: &mut App,
    group: &str,
    pattern: &str,
    start_ms: i64,
    end_ms: i64,
    initial_token: Option<String>,
) {
    let cw_client = match &app.aws {
        Some(a) => a.cloudwatch.clone(),
        None => return,
    };

    let mut token = initial_token;
    // Do at most 3 API calls per burst — enough to skip empties but fast enough
    // to return control to the event loop for scroll/input handling
    for _ in 0..3 {
        match aws::cloudwatch::filter_log_group_events(
            &cw_client,
            group,
            pattern,
            start_ms,
            end_ms,
            token.as_deref(),
        )
        .await
        {
            Ok(result) => {
                if !result.events.is_empty() {
                    app.cloudwatch_view.events.extend(result.events);
                }
                token = result.next_token;
                app.cloudwatch_view.next_token = token.clone();

                if token.is_none() {
                    // All done — no more pages
                    app.cloudwatch_view.loading = false;
                    app.cloudwatch_view.search_continuing = false;
                    return;
                }
            }
            Err(e) => {
                if app.cloudwatch_view.events.is_empty() {
                    app.cloudwatch_view.set_error(e);
                } else {
                    app.cloudwatch_view.next_token = None;
                    app.cloudwatch_view.loading = false;
                }
                app.cloudwatch_view.search_continuing = false;
                return;
            }
        }
    }

    // Burst done, more pages remain.
    // If we still have 0 events, keep search_continuing=true so tick drives the next burst.
    // If we have events, let auto-scroll drive further loads.
    if app.cloudwatch_view.events.is_empty() {
        app.cloudwatch_view.search_continuing = true;
        // Keep loading=true so the UI shows "loading..."
    } else {
        app.cloudwatch_view.search_continuing = false;
        app.cloudwatch_view.loading = false;
    }
}

async fn handle_cw_insights(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
) {
    let groups = app.cloudwatch_view.insights_groups.clone();
    let query = app.cloudwatch_view.insights_query.clone();
    let (start_secs, end_secs) = app.cloudwatch_view.insights_time_secs();

    let _ = terminal.draw(|frame| ui::render(app, frame));

    let cw_client = match &app.aws {
        Some(a) => a.cloudwatch.clone(),
        None => return,
    };

    let query_id = match aws::cloudwatch::start_insights_query(
        &cw_client,
        &groups,
        &query,
        start_secs,
        end_secs,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            app.cloudwatch_view.insights_status = Some(format!("Error: {}", e));
            app.cloudwatch_view.loading = false;
            return;
        }
    };

    app.cloudwatch_view.insights_status = Some("Running...".to_string());
    let _ = terminal.draw(|frame| ui::render(app, frame));

    // Poll until complete
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        match aws::cloudwatch::get_insights_results(&cw_client, &query_id).await {
            Ok(result) => {
                match result.status.as_str() {
                    "Complete" => {
                        let events: Vec<aws::cloudwatch::LogEvent> = result
                            .rows
                            .iter()
                            .map(|row| {
                                let ts_str = row.get("@timestamp").map(|s| s.as_str()).unwrap_or("");
                                let msg = row.get("@message").map(|s| s.as_str()).unwrap_or("");
                                let timestamp = parse_insights_timestamp(ts_str);
                                aws::cloudwatch::LogEvent {
                                    timestamp,
                                    message: msg.to_string(),
                                }
                            })
                            .collect();
                        let count = events.len();
                        app.cloudwatch_view.insights_status = Some(format!("Complete — {} results", count));
                        app.cloudwatch_view.enter_insights_results(events);
                        return;
                    }
                    "Failed" | "Cancelled" | "Timeout" => {
                        app.cloudwatch_view.insights_status =
                            Some(format!("Query {}", result.status));
                        app.cloudwatch_view.loading = false;
                        let _ = terminal.draw(|frame| ui::render(app, frame));
                        return;
                    }
                    _ => {
                        app.cloudwatch_view.insights_status =
                            Some(format!("Running... ({} rows so far)", result.rows.len()));
                        let _ = terminal.draw(|frame| ui::render(app, frame));
                    }
                }
            }
            Err(e) => {
                app.cloudwatch_view.insights_status = Some(format!("Error: {}", e));
                app.cloudwatch_view.loading = false;
                let _ = terminal.draw(|frame| ui::render(app, frame));
                return;
            }
        }
    }
}

fn parse_insights_timestamp(ts_str: &str) -> i64 {
    // Try to parse ISO 8601 format from Insights (e.g. "2026-03-15 21:48:30.486")
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S%.f") {
        return dt.and_utc().timestamp_millis();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S") {
        return dt.and_utc().timestamp_millis();
    }
    0
}

async fn handle_secrets_enter(app: &mut App) {
    if app.secrets_view.needs_secret_load() {
        return;
    }

    match app.secrets_view.screen_type() {
        "list" => {
            if let Some(secret) = app.secrets_view.selected_secret().cloned() {
                app.secrets_view.enter_detail();
                if let Some(ref aws) = app.aws {
                    match aws::secrets::get_secret_detail(&aws.secrets, &secret.name).await {
                        Ok(detail) => app.secrets_view.set_detail(detail),
                        Err(e) => app.secrets_view.set_error(e),
                    }
                }
            }
        }
        _ => {}
    }
}

async fn handle_lambda_enter(app: &mut App) {
    if app.lambda_view.needs_function_load() {
        return;
    }

    match app.lambda_view.screen_type() {
        "functions" => {
            if let Some(func) = app.lambda_view.selected_function().cloned() {
                app.lambda_view.enter_detail();
                if let Some(ref aws) = app.aws {
                    match aws::lambda::get_function_detail(&aws.lambda, &func.name).await {
                        Ok(detail) => app.lambda_view.set_detail(detail),
                        Err(e) => app.lambda_view.set_error(e),
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
    // Try pbcopy (macOS), then xclip, then xsel (Linux)
    let commands = [
        ("pbcopy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
    ];
    for (cmd, args) in &commands {
        if let Ok(mut child) = Command::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(std::path::PathBuf::from)
}

async fn handle_ddb_enter(app: &mut App) {
    match app.dynamodb_view.screen_type() {
        "tables" => {
            if let Some(table) = app.dynamodb_view.selected_table().cloned() {
                app.dynamodb_view.enter_table(table.name.clone());
                if let Some(ref aws) = app.aws {
                    if let Ok(detail) =
                        aws::dynamodb::describe_table(&aws.dynamodb, &table.name).await
                    {
                        app.dynamodb_view.set_table_detail(detail);
                    }
                    match aws::dynamodb::scan_table(&aws.dynamodb, &table.name, None, 300, None, None)
                        .await
                    {
                        Ok(result) => app.dynamodb_view.set_items(result),
                        Err(e) => app.dynamodb_view.set_error(e),
                    }
                }
            }
        }
        "items" => {
            app.dynamodb_view.enter_detail();
        }
        _ => {}
    }
}

async fn handle_ddb_run_query(app: &mut App) {
    let table = app.dynamodb_view.active_table.clone();
    let index = app.dynamodb_view.active_index.clone();
    let ddb = &app.dynamodb_view;

    if ddb.query_mode && !ddb.query_pk_value.is_empty() {
        // Query mode
        let detail = match ddb.table_detail() {
            Some(d) => d,
            None => return,
        };

        let (pk_name, sk_name) = if let Some(ref idx_name) = index {
            if let Some(idx) = detail.indexes.iter().find(|i| &i.name == idx_name) {
                (idx.partition_key.clone(), idx.sort_key.clone())
            } else {
                (detail.partition_key.clone(), detail.sort_key.clone())
            }
        } else {
            (detail.partition_key.clone(), detail.sort_key.clone())
        };

        let pk_value = ddb.query_pk_value.clone();
        let sk_condition = ddb.query_sk_condition.clone();
        let sk_value = ddb.query_sk_value.clone();
        let descending = ddb.query_descending;

        let has_sk_query = sk_name.is_some() && !sk_value.is_empty();
        let sk_n_str = sk_name.clone();
        let sk_c_str = sk_condition.clone();
        let sk_v_str = sk_value.clone();
        let filters = ddb.filter_tuples();

        if let Some(ref aws) = app.aws {
            let filter_ref = if filters.is_empty() { None } else { Some(filters.as_slice()) };
            match aws::dynamodb::query_table(
                &aws.dynamodb,
                &table,
                index.as_deref(),
                &pk_name,
                &pk_value,
                if has_sk_query { sk_n_str.as_deref() } else { None },
                if has_sk_query { Some(sk_c_str.as_str()) } else { None },
                if has_sk_query { Some(sk_v_str.as_str()) } else { None },
                !descending,
                300,
                filter_ref,
            )
            .await
            {
                Ok(result) => {
                    let mut summary = format!("{} = \"{}\"", &pk_name, &pk_value);
                    if has_sk_query {
                        if let Some(ref skn) = sk_n_str {
                            summary.push_str(&format!(" | {} {} \"{}\"", skn, &sk_c_str, &sk_v_str));
                        }
                    }
                    for (a, c, v) in &filters {
                        summary.push_str(&format!(" | {} {} \"{}\"", a, c, v));
                    }
                    app.dynamodb_view.set_items(result);
                    app.dynamodb_view.query_summary = Some(summary);
                }
                Err(e) => app.dynamodb_view.set_error(e),
            }
        }
    } else {
        // Scan mode — pass filters if any
        let filters = app.dynamodb_view.filter_tuples();
        let filter_ref = if filters.is_empty() { None } else { Some(filters.as_slice()) };
        if let Some(ref aws) = app.aws {
            match aws::dynamodb::scan_table(
                &aws.dynamodb,
                &table,
                index.as_deref(),
                300,
                None,
                filter_ref,
            )
            .await
            {
                Ok(result) => {
                    let summary = if !filters.is_empty() {
                        Some(filters.iter().map(|(a, c, v)| format!("{} {} \"{}\"", a, c, v)).collect::<Vec<_>>().join(" | "))
                    } else {
                        None
                    };
                    app.dynamodb_view.set_items(result);
                    app.dynamodb_view.query_summary = summary;
                }
                Err(e) => app.dynamodb_view.set_error(e),
            }
        }
    }
}

async fn handle_ddb_switch_index(app: &mut App) {
    let table = app.dynamodb_view.active_table.clone();
    let index = app.dynamodb_view.active_index.clone();
    if let Some(ref aws) = app.aws {
        match aws::dynamodb::scan_table(
            &aws.dynamodb,
            &table,
            index.as_deref(),
            300,
            None,
            None,
        )
        .await
        {
            Ok(result) => app.dynamodb_view.set_items(result),
            Err(e) => app.dynamodb_view.set_error(e),
        }
    }
}

async fn handle_ddb_next_page(app: &mut App) {
    let table = app.dynamodb_view.active_table.clone();
    let index = app.dynamodb_view.active_index.clone();
    let last_key = app.dynamodb_view.last_key().cloned();

    if let (Some(aws), Some(start_key)) = (&app.aws, &last_key) {
        match aws::dynamodb::scan_table(
            &aws.dynamodb,
            &table,
            index.as_deref(),
            300,
            Some(&start_key),
            None,
        )
        .await
        {
            Ok(result) => app.dynamodb_view.add_page(result),
            Err(e) => app.dynamodb_view.set_error(e),
        }
    }
}

async fn handle_ddb_back(app: &mut App) {
    if app.dynamodb_view.needs_items_load() {
        let table = app.dynamodb_view.active_table.clone();
        let index = app.dynamodb_view.active_index.clone();
        if let Some(ref aws) = app.aws {
            match aws::dynamodb::scan_table(
                &aws.dynamodb,
                &table,
                index.as_deref(),
                300,
                None,
                None,
            )
            .await
            {
                Ok(result) => app.dynamodb_view.set_items(result),
                Err(e) => app.dynamodb_view.set_error(e),
            }
        }
    }
}

fn handle_normal_key(code: KeyCode, modifiers: KeyModifiers, app: &App) -> Action {
    match (code, modifiers) {
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
