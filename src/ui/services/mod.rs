pub mod cloudwatch;
pub mod dynamodb;
pub mod lambda;
pub mod s3;
pub mod secrets;

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::Action;

pub trait ServiceComponent {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action>;
    fn render(&mut self, frame: &mut Frame, area: Rect);
    fn name(&self) -> &'static str;
}
