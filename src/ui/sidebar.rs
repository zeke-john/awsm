use ratatui::layout::Rect;
use ratatui::Frame;

use crate::keys::Service;

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
    pub fn render(&self, _frame: &mut Frame, _area: Rect) {
    }
}
