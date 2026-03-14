pub mod popup;
pub mod search;
pub mod services;
pub mod sidebar;
pub mod statusbar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(1)])
        .split(outer[0]);

    app.sidebar.render(frame, body[0], app.focus, app.active_service);

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

    let placeholder = Paragraph::new(Span::styled(
        "  No data yet",
        Style::default().fg(Color::DarkGray),
    ))
    .block(main_block);

    frame.render_widget(placeholder, body[1]);

    statusbar::StatusBar::render(frame, outer[1], app.mode, app.active_service);

    if app.show_help {
        popup::render_help(frame);
    }
}
