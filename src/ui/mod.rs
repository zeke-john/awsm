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

pub fn render(app: &App, frame: &mut Frame) {
    match app.screen {
        Screen::ProfilePicker => render_profile_picker(app, frame),
        Screen::Main => render_main(app, frame),
    }
}

fn render_profile_picker(app: &App, frame: &mut Frame) {
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
            let marker = if is_selected { " ▸ " } else { "   " };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(name.as_str(), style),
            ]))
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default().with_selected(Some(app.profile_selected));
    frame.render_stateful_widget(list, inner[1], &mut state);
}

fn render_main(app: &App, frame: &mut Frame) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(1)])
        .split(outer[0]);

    app.sidebar
        .render(frame, body[0], app.focus, app.active_service);

    let main_border = if app.focus == crate::keys::Focus::Main {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let main_block = Block::default()
        .title(format!(" {} ", app.active_service.label()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(main_border);

    let main_text = if app.aws.is_some() {
        "  Ready"
    } else {
        "  Connecting to AWS..."
    };

    let placeholder = Paragraph::new(Span::styled(
        main_text,
        Style::default().fg(Color::DarkGray),
    ))
    .block(main_block);

    frame.render_widget(placeholder, body[1]);

    statusbar::StatusBar::render(frame, outer[1], app);

    if app.show_help {
        popup::render_help(frame);
    }
}
