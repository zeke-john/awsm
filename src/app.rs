use crate::keys::{Focus, Mode, Service};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchService(Service),
    SetMode(Mode),
    SetFocus(Focus),
    None,
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub mode: Mode,
    pub active_service: Service,
    pub focus: Focus,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            mode: Mode::default(),
            active_service: Service::default(),
            focus: Focus::default(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn update(&mut self, action: Action) {
        match action {
            Action::Quit => self.quit(),
            Action::SwitchService(service) => self.active_service = service,
            Action::SetMode(mode) => self.mode = mode,
            Action::SetFocus(focus) => self.focus = focus,
            Action::None => {}
        }
    }
}
