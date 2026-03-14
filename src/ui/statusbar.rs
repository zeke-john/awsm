use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::keys::{Mode, Service};

pub struct StatusBar;

impl StatusBar {
    pub fn render(frame: &mut Frame, area: Rect, mode: Mode, service: Service) {
        let (mode_label, mode_color) = match mode {
            Mode::Normal => (" NORMAL ", Color::Blue),
            Mode::Insert => (" INSERT ", Color::Green),
            Mode::Command => (" COMMAND ", Color::Yellow),
        };

        let left = Line::from(vec![
            Span::styled(
                mode_label,
                Style::default()
                    .fg(Color::Black)
                    .bg(mode_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!(" {} ", service.label()),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
            Span::styled(
                " │ ?:help q:quit ",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let bar = Paragraph::new(left).alignment(Alignment::Left);
        frame.render_widget(bar, area);
    }
}
