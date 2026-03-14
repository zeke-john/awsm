use ratatui::layout::Rect;
use ratatui::Frame;

#[derive(Debug, Default)]
pub struct StatusBar;

impl StatusBar {
    pub fn render(&self, _frame: &mut Frame, _area: Rect) {
    }
}
