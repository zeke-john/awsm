use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub struct StatusBar;

impl StatusBar {
    pub fn render(frame: &mut Frame, area: Rect, app: &App) {
        let (mode_label, mode_color) = match app.mode {
            crate::keys::Mode::Normal => (" NORMAL ", Color::Blue),
            crate::keys::Mode::Insert => (" INSERT ", Color::Green),
            crate::keys::Mode::Command => (" COMMAND ", Color::Yellow),
        };

        let region_display = if app.region.is_empty() {
            "no region".to_string()
        } else {
            app.region.clone()
        };

        let mut spans = vec![
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
            spans.push(Span::styled(" ", Style::default()));
            spans.push(Span::styled(
                format!(" {} ", err),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        spans.push(Span::styled(
            " │ ? help  q quit ",
            Style::default().fg(Color::DarkGray),
        ));

        let bar = Paragraph::new(Line::from(spans));
        frame.render_widget(bar, area);
    }
}
