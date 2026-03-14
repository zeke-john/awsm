use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::keys::{Focus, Service};

#[derive(Debug)]
pub struct Sidebar {
    pub selected: usize,
    pub services: Vec<Service>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            selected: 0,
            services: Service::ALL.to_vec(),
        }
    }
}

impl Sidebar {
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.services.len() {
            self.selected += 1;
        }
    }

    pub fn selected_service(&self) -> Option<Service> {
        self.services.get(self.selected).copied()
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, focus: Focus, active: Service) {
        let focused = focus == Focus::Sidebar;

        let border_style = if focused {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Services ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let items: Vec<ListItem> = self
            .services
            .iter()
            .enumerate()
            .map(|(i, service)| {
                let is_active = *service == active;
                let is_selected = i == self.selected && focused;

                let marker = if is_active { "▸ " } else { "  " };
                let label = service.label();

                let style = if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else if is_active {
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(label, style),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}
