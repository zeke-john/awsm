use crate::keys::{Focus, Mode, Service};
use crate::ui::sidebar::Sidebar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    SwitchService(Service),
    SetMode(Mode),
    SetFocus(Focus),
    ToggleFocus,
    SidebarUp,
    SidebarDown,
    SelectService,
    ToggleHelp,
    None,
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub mode: Mode,
    pub active_service: Service,
    pub focus: Focus,
    pub sidebar: Sidebar,
    pub show_help: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            mode: Mode::default(),
            active_service: Service::default(),
            focus: Focus::default(),
            sidebar: Sidebar::default(),
            show_help: false,
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
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::Main,
                    Focus::Main => Focus::Sidebar,
                };
            }
            Action::SidebarUp => self.sidebar.move_up(),
            Action::SidebarDown => self.sidebar.move_down(),
            Action::SelectService => {
                if let Some(service) = self.sidebar.selected_service() {
                    self.active_service = service;
                    self.focus = Focus::Main;
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::None => {}
        }
    }
}
