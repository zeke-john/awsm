pub mod popup;
pub mod search;
pub mod services;
pub mod sidebar;
pub mod statusbar;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Screen};
use crate::keys::Service;
use crate::ui::services::ServiceComponent;

/// Syntax-highlight a pretty-printed JSON string into styled ratatui Lines.
pub fn highlight_json(json: &str) -> Vec<Line<'static>> {
    let key_style = Style::default().fg(Color::Cyan);
    let string_style = Style::default().fg(Color::Green);
    let number_style = Style::default().fg(Color::Yellow);
    let bool_null_style = Style::default().fg(Color::Magenta);
    let brace_style = Style::default().fg(Color::White);
    let punct_style = Style::default().fg(Color::DarkGray);

    json.lines()
        .map(|line| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let chars: Vec<char> = line.chars().collect();
            let len = chars.len();
            let mut i = 0;

            // leading whitespace
            while i < len && chars[i] == ' ' {
                i += 1;
            }
            if i > 0 {
                spans.push(Span::raw(" ".repeat(i)));
            }

            // track whether first string on this line is a key (followed by `:`)
            let mut seen_key = false;

            while i < len {
                let ch = chars[i];
                match ch {
                    '"' => {
                        // collect the full string including quotes
                        let start = i;
                        i += 1;
                        while i < len && chars[i] != '"' {
                            if chars[i] == '\\' {
                                i += 1; // skip escaped char
                            }
                            i += 1;
                        }
                        if i < len {
                            i += 1; // closing quote
                        }
                        let s: String = chars[start..i].iter().collect();

                        // determine if this is a key: look ahead for `:`
                        let mut j = i;
                        while j < len && chars[j] == ' ' {
                            j += 1;
                        }
                        let is_key = !seen_key && j < len && chars[j] == ':';
                        if is_key {
                            spans.push(Span::styled(s, key_style));
                            seen_key = true;
                        } else {
                            spans.push(Span::styled(s, string_style));
                        }
                    }
                    '{' | '}' | '[' | ']' => {
                        spans.push(Span::styled(String::from(ch), brace_style));
                        i += 1;
                    }
                    ':' | ',' => {
                        spans.push(Span::styled(String::from(ch), punct_style));
                        i += 1;
                    }
                    _ if ch.is_ascii_digit() || ch == '-' || ch == '.' => {
                        let start = i;
                        while i < len
                            && (chars[i].is_ascii_digit()
                                || chars[i] == '.'
                                || chars[i] == '-'
                                || chars[i] == 'e'
                                || chars[i] == 'E'
                                || chars[i] == '+')
                        {
                            i += 1;
                        }
                        let s: String = chars[start..i].iter().collect();
                        spans.push(Span::styled(s, number_style));
                    }
                    't' | 'f' | 'n' => {
                        // true, false, null
                        let start = i;
                        while i < len && chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let s: String = chars[start..i].iter().collect();
                        spans.push(Span::styled(s, bool_null_style));
                    }
                    ' ' => {
                        let start = i;
                        while i < len && chars[i] == ' ' {
                            i += 1;
                        }
                        spans.push(Span::raw(" ".repeat(i - start)));
                    }
                    _ => {
                        spans.push(Span::raw(String::from(ch)));
                        i += 1;
                    }
                }
            }

            Line::from(spans)
        })
        .collect()
}

pub fn render(app: &mut App, frame: &mut Frame) {
    match app.screen {
        Screen::ProfilePicker => render_profile_picker(app, frame),
        Screen::Main => render_main(app, frame),
    }
}

fn render_profile_picker(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let profile_count = app.available_profiles.len() as u16;
    let list_height = profile_count + 2;

    let vertical = Layout::vertical([Constraint::Length(list_height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(50)])
        .flex(Flex::Center)
        .split(vertical[0]);

    let picker_area = horizontal[0];

    let title = Paragraph::new(Line::from(vec![Span::styled(
        "Select AWS Profile",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(ratatui::layout::Alignment::Center);

    let inner = Layout::vertical([Constraint::Length(2), Constraint::Length(profile_count)])
        .split(picker_area);

    frame.render_widget(title, inner[0]);

    let items: Vec<ListItem> = app
        .available_profiles
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let is_selected = i == app.profile_selected;
            if is_selected {
                let style = Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD);
                ListItem::new(Line::from(vec![
                    Span::styled(" › ", style),
                    Span::styled(name.as_str(), style),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled("   ", Style::default().fg(Color::DarkGray)),
                    Span::styled(name.as_str(), Style::default().fg(Color::DarkGray)),
                ]))
            }
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default().with_selected(Some(app.profile_selected));
    frame.render_stateful_widget(list, inner[1], &mut state);
}

fn render_main(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 40 || area.height < 8 {
        let msg = Paragraph::new(Span::styled(
            "Terminal too small pls resize to at least 40x8",
            Style::default().fg(Color::Yellow),
        ));
        frame.render_widget(msg, area);
        return;
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let main_area = if app.show_sidebar {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(22), Constraint::Min(1)])
            .split(outer[0]);

        app.sidebar
            .render(frame, body[0], app.focus, app.active_service);
        body[1]
    } else {
        outer[0]
    };

    let main_border = if app.focus == crate::keys::Focus::Main {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = match app.active_service {
        Service::S3 => format!(" {} ", app.s3_view.breadcrumb()),
        Service::DynamoDB => format!(" {} ", app.dynamodb_view.breadcrumb()),
        Service::Lambda => format!(" {} ", app.lambda_view.breadcrumb()),
        Service::CloudWatch => format!(" {} ", app.cloudwatch_view.breadcrumb()),
        Service::SecretsManager => format!(" {} ", app.secrets_view.breadcrumb()),
    };

    let main_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(main_border);

    let inner = main_block.inner(main_area);
    frame.render_widget(main_block, main_area);

    match app.active_service {
        Service::S3 => {
            app.s3_view.render(frame, inner);
        }
        Service::DynamoDB => {
            app.dynamodb_view.render(frame, inner);
        }
        Service::Lambda => {
            app.lambda_view.render(frame, inner);
        }
        Service::CloudWatch => {
            app.cloudwatch_view.render(frame, inner);
        }
        Service::SecretsManager => {
            app.secrets_view.render(frame, inner);
        }
    }

    statusbar::StatusBar::render(frame, outer[1], app);

    if app.show_help {
        popup::render_help(frame, &mut app.help_scroll);
    }
}
