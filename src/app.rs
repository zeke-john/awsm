use crate::aws::AwsClients;
use crate::keys::{Focus, Mode, Service};
use crate::ui::services::dynamodb::DynamoDbView;
use crate::ui::services::lambda::LambdaView;
use crate::ui::services::s3::S3View;
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
    ProfileUp,
    ProfileDown,
    ProfileSelect,
    ServiceEnter,
    ServiceBack,
    S3Download,
    S3CopyUri,
    S3CopyArn,
    DdbNextPage,
    DdbSwitchIndex,
    DdbRunQuery,
    ToggleSidebar,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    ProfilePicker,
    Main,
}

pub struct App {
    pub running: bool,
    pub screen: Screen,
    pub mode: Mode,
    pub active_service: Service,
    pub focus: Focus,
    pub sidebar: Sidebar,
    pub show_help: bool,
    pub help_scroll: u16,
    pub show_sidebar: bool,
    pub aws: Option<AwsClients>,
    pub aws_error: Option<String>,
    pub region: String,
    pub profile: String,
    pub available_profiles: Vec<String>,
    pub profile_selected: usize,
    pub s3_view: S3View,
    pub dynamodb_view: DynamoDbView,
    pub lambda_view: LambdaView,
    pub command_input: String,
}

impl App {
    pub fn new() -> Self {
        let profiles = crate::aws::list_profiles();
        let env_profile = std::env::var("AWS_PROFILE").ok();

        let (screen, profile, profile_selected) = if let Some(ref p) = env_profile {
            (Screen::Main, p.clone(), 0)
        } else if profiles.len() == 1 {
            (Screen::Main, profiles[0].clone(), 0)
        } else if profiles.is_empty() {
            (Screen::Main, "default".to_string(), 0)
        } else {
            (Screen::ProfilePicker, String::new(), 0)
        };

        Self {
            running: true,
            screen,
            mode: Mode::default(),
            active_service: Service::default(),
            focus: Focus::default(),
            sidebar: Sidebar::default(),
            show_help: false,
            help_scroll: 0,
            show_sidebar: true,
            aws: None,
            aws_error: None,
            region: String::new(),
            profile,
            available_profiles: profiles,
            profile_selected,
            s3_view: S3View::default(),
            dynamodb_view: DynamoDbView::default(),
            lambda_view: LambdaView::default(),
            command_input: String::new(),
        }
    }

    pub async fn init_aws(&mut self) {
        let clients = AwsClients::new(&self.profile).await;
        self.region = clients.region();
        self.aws = Some(clients);
        self.aws_error = None;
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
            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                if self.show_help {
                    self.help_scroll = 0;
                }
            }
            Action::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                if !self.show_sidebar {
                    self.focus = Focus::Main;
                }
            }
            Action::ProfileUp => {
                if self.profile_selected > 0 {
                    self.profile_selected -= 1;
                }
            }
            Action::ProfileDown => {
                if self.profile_selected + 1 < self.available_profiles.len() {
                    self.profile_selected += 1;
                }
            }
            Action::ProfileSelect => {
                if let Some(p) = self.available_profiles.get(self.profile_selected) {
                    self.profile = p.clone();
                    self.screen = Screen::Main;
                }
            }
            Action::ServiceEnter | Action::ServiceBack
            | Action::S3Download | Action::S3CopyUri | Action::S3CopyArn
            | Action::DdbNextPage | Action::DdbSwitchIndex | Action::DdbRunQuery
            | Action::None => {}
        }
    }
}
