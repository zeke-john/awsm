use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub struct StatusBar;

impl StatusBar {
    pub fn render(frame: &mut Frame, area: Rect, app: &App) {
        let is_editing = match app.active_service {
            crate::keys::Service::S3 => app.s3_view.is_editing(),
            crate::keys::Service::DynamoDB => app.dynamodb_view.is_editing(),
            _ => false,
        };

        let (mode_label, mode_color) = if is_editing {
            (" INSERT ", Color::Green)
        } else {
            match app.mode {
                crate::keys::Mode::Normal => (" NORMAL ", Color::Blue),
                crate::keys::Mode::Insert => (" INSERT ", Color::Green),
                crate::keys::Mode::Command => (" COMMAND ", Color::Yellow),
            }
        };

        let region_display = if app.region.is_empty() {
            "no region".to_string()
        } else {
            app.region.clone()
        };

        let mut left_spans = vec![
            Span::styled(
                mode_label,
                Style::default()
                    .fg(Color::Black)
                    .bg(mode_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!(" {} ", region_display),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!(" {} ", app.profile),
                Style::default().fg(Color::Cyan).bg(Color::DarkGray),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!(" {} ", app.active_service.label()),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
        ];

        if let Some(ref err) = app.aws_error {
            left_spans.push(Span::styled(" ", Style::default()));
            left_spans.push(Span::styled(
                format!(" {} ", err),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(20)])
            .split(area);

        let left = Paragraph::new(Line::from(left_spans));
        frame.render_widget(left, cols[0]);

        let right = Paragraph::new(Line::from(vec![
            Span::styled("? ", Style::default().fg(Color::DarkGray)),
            Span::styled("help  ", Style::default().fg(Color::DarkGray)),
            Span::styled("^c ", Style::default().fg(Color::DarkGray)),
            Span::styled("quit ", Style::default().fg(Color::DarkGray)),
        ]))
        .alignment(ratatui::layout::Alignment::Right);
        frame.render_widget(right, cols[1]);
    }
}
