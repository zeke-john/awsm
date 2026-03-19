use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::Action;
use crate::aws::cloudwatch::{LogEvent, LogGroupInfo, LogStreamInfo, format_epoch_ms, format_stored_bytes};
use crate::ui::services::ServiceComponent;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CwScreen {
    Groups,
    Streams,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupSortCol {
    Name,
    StoredBytes,
    Retention,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamSortCol {
    Name,
    LastEvent,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
}

fn parse_hours(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(3.0).max(0.01)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventsSource {
    Stream,
    Search,
    Insights,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchField {
    Pattern,
    TimeRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsightsField {
    Query,
    TimeRange,
}

impl SortDir {
    fn toggle(&self) -> Self {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }

    fn arrow(&self) -> &str {
        match self {
            SortDir::Asc => "▲",
            SortDir::Desc => "▼",
        }
    }
}

#[derive(Debug)]
pub struct CloudWatchView {
    screen: CwScreen,
    // Groups
    groups: Vec<LogGroupInfo>,
    group_selected: usize,
    group_list_state: ListState,
    group_sort_col: GroupSortCol,
    group_sort_dir: SortDir,
    selected_groups: HashSet<String>,
    // Streams
    pub active_group: String,
    streams: Vec<LogStreamInfo>,
    stream_selected: usize,
    stream_list_state: ListState,
    stream_sort_col: StreamSortCol,
    stream_sort_dir: SortDir,
    // Events — newest first, `n` loads older
    pub active_stream: String,
    pub events: Vec<LogEvent>,
    pub next_token: Option<String>,
    event_scroll: usize,
    event_visible_rows: usize,
    // Shared
    pub loading: bool,
    error: Option<String>,
    filter: String,
    filtering: bool,
    pending_g: bool,
    pub refresh_flash: u8,
    // Search popup
    pub search_popup_open: bool,
    pub search_pattern: String,
    pub search_hours: String,
    search_field: SearchField,
    search_editing: bool,
    search_hours_editing: bool,
    pub search_status: Option<String>,
    // Insights popup
    pub insights_popup_open: bool,
    pub insights_query: String,
    pub insights_hours: String,
    insights_field: InsightsField,
    insights_editing: bool,
    insights_hours_editing: bool,
    pub insights_status: Option<String>,
    pub insights_groups: Vec<String>,
    // Events source tracking
    events_source: EventsSource,
    return_screen: CwScreen,
    pub search_pattern_display: String,
    pub search_continuing: bool,
}

impl Default for CloudWatchView {
    fn default() -> Self {
        Self {
            screen: CwScreen::Groups,
            groups: Vec::new(),
            group_selected: 0,
            group_list_state: ListState::default(),
            group_sort_col: GroupSortCol::Name,
            group_sort_dir: SortDir::Asc,
            selected_groups: HashSet::new(),
            active_group: String::new(),
            streams: Vec::new(),
            stream_selected: 0,
            stream_list_state: ListState::default(),
            stream_sort_col: StreamSortCol::LastEvent,
            stream_sort_dir: SortDir::Desc,
            active_stream: String::new(),
            events: Vec::new(),
            next_token: None,
            event_scroll: 0,
            event_visible_rows: 40,
            loading: true,
            error: None,
            filter: String::new(),
            filtering: false,
            pending_g: false,
            refresh_flash: 0,
            search_popup_open: false,
            search_pattern: String::new(),
            search_hours: "3".to_string(),
            search_field: SearchField::Pattern,
            search_editing: false,
            search_hours_editing: false,
            search_status: None,
            insights_popup_open: false,
            insights_query: "fields @timestamp, @message\n| sort @timestamp desc\n| limit 200".to_string(),
            insights_hours: "3".to_string(),
            insights_field: InsightsField::Query,
            insights_editing: false,
            insights_hours_editing: false,
            insights_status: None,
            insights_groups: Vec::new(),
            events_source: EventsSource::Stream,
            return_screen: CwScreen::Groups,
            search_pattern_display: String::new(),
            search_continuing: false,
        }
    }
}

impl CloudWatchView {
    pub fn set_groups(&mut self, groups: Vec<LogGroupInfo>) {
        self.groups = groups;
        self.loading = false;
        self.error = None;
        self.group_selected = 0;
        self.group_list_state = ListState::default();
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    pub fn enter_group(&mut self, name: String) {
        self.active_group = name;
        self.screen = CwScreen::Streams;
        self.streams.clear();
        self.stream_selected = 0;
        self.loading = true;
        self.error = None;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn set_streams(&mut self, streams: Vec<LogStreamInfo>) {
        self.streams = streams;
        self.loading = false;
        self.error = None;
        self.stream_selected = 0;
        self.stream_list_state = ListState::default();
    }

    pub fn enter_stream(&mut self, name: String) {
        self.active_stream = name;
        self.screen = CwScreen::Events;
        self.events.clear();
        self.next_token = None;
        self.event_scroll = 0;
        self.loading = true;
        self.error = None;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn set_events(&mut self, events: Vec<LogEvent>, next_token: Option<String>) {
        self.events = events;
        self.next_token = next_token;
        self.loading = false;
        self.error = None;
        self.event_scroll = 0;
    }

    pub fn append_events(&mut self, events: Vec<LogEvent>, next_token: Option<String>) {
        self.events.extend(events);
        self.next_token = next_token;
        self.loading = false;
    }

    pub fn go_back(&mut self) -> bool {
        if self.filtering {
            self.filtering = false;
            self.filter.clear();
            return true;
        }
        self.filter.clear();
        self.filtering = false;
        match self.screen {
            CwScreen::Events => {
                self.search_continuing = false;
                if self.events_source != EventsSource::Stream {
                    self.screen = self.return_screen.clone();
                    self.events.clear();
                    self.events_source = EventsSource::Stream;
                    self.loading = false;
                } else {
                    self.screen = CwScreen::Streams;
                    self.events.clear();
                    self.loading = false;
                }
                true
            }
            CwScreen::Streams => {
                self.screen = CwScreen::Groups;
                self.streams.clear();
                self.loading = false;
                true
            }
            CwScreen::Groups => false,
        }
    }

    pub fn selected_group(&self) -> Option<&LogGroupInfo> {
        let filtered = self.filtered_groups();
        filtered.into_iter().nth(self.group_selected)
    }

    pub fn selected_stream(&self) -> Option<&LogStreamInfo> {
        let filtered = self.filtered_streams();
        filtered.into_iter().nth(self.stream_selected)
    }

    pub fn is_editing(&self) -> bool {
        self.filtering || self.search_editing || self.search_hours_editing
            || self.insights_editing || self.insights_hours_editing
    }

    pub fn has_overlay(&self) -> bool {
        self.search_popup_open || self.insights_popup_open
    }

    pub fn open_search_popup(&mut self) {
        self.search_popup_open = true;
        self.search_pattern.clear();
        self.search_field = SearchField::Pattern;
        self.search_editing = false;
        self.search_hours_editing = false;
        self.search_status = None;
        self.return_screen = self.screen.clone();
    }

    pub fn open_insights_popup(&mut self) {
        self.insights_popup_open = true;
        self.insights_field = InsightsField::Query;
        self.insights_editing = false;
        self.insights_hours_editing = false;
        self.insights_status = None;
        self.return_screen = self.screen.clone();
    }

    /// Transition to events screen for search — called before streaming results in
    pub fn start_search_view(&mut self) {
        self.search_popup_open = false;
        self.search_editing = false;
        self.search_hours_editing = false;
        self.search_pattern_display = self.search_pattern.clone();
        self.events_source = EventsSource::Search;
        self.screen = CwScreen::Events;
        self.events.clear();
        self.next_token = None;
        self.event_scroll = 0;
        self.loading = true;
        self.error = None;
        self.filter.clear();
        self.filtering = false;
        self.search_continuing = true;
    }

    pub fn enter_insights_results(&mut self, events: Vec<LogEvent>) {
        self.insights_popup_open = false;
        self.insights_editing = false;
        self.events_source = EventsSource::Insights;
        self.screen = CwScreen::Events;
        self.events = events;
        self.next_token = None;
        self.event_scroll = 0;
        self.loading = false;
        self.error = None;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn search_time_millis(&self) -> (i64, i64) {
        let hours = parse_hours(&self.search_hours);
        let millis = (hours * 3_600_000.0) as i64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        (now - millis, now)
    }

    pub fn insights_time_secs(&self) -> (i64, i64) {
        let hours = parse_hours(&self.insights_hours);
        let secs = (hours * 3600.0) as i64;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        (now - secs, now)
    }

    pub fn needs_group_load(&self) -> bool {
        self.screen == CwScreen::Groups && self.loading
    }

    pub fn screen_type(&self) -> &str {
        match self.screen {
            CwScreen::Groups => "groups",
            CwScreen::Streams => "streams",
            CwScreen::Events => "events",
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.screen {
            CwScreen::Groups => "CloudWatch > Log Groups".to_string(),
            CwScreen::Streams => {
                let name = shorten_group_name(&self.active_group);
                format!("CloudWatch > {}", name)
            }
            CwScreen::Events => {
                let group = shorten_group_name(&self.active_group);
                match self.events_source {
                    EventsSource::Stream => {
                        let stream = truncate_str(&self.active_stream, 30);
                        format!("CloudWatch > {} > {}", group, stream)
                    }
                    EventsSource::Search => {
                        format!("CloudWatch > {} > Search", group)
                    }
                    EventsSource::Insights => {
                        if self.insights_groups.len() > 1 {
                            format!("CloudWatch > All Groups > Insights")
                        } else {
                            format!("CloudWatch > {} > Insights", group)
                        }
                    }
                }
            }
        }
    }

    fn filtered_groups(&self) -> Vec<&LogGroupInfo> {
        let mut result: Vec<&LogGroupInfo> = if self.filter.is_empty() {
            self.groups.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.groups
                .iter()
                .filter(|g| g.name.to_lowercase().contains(&f))
                .collect()
        };

        result.sort_by(|a, b| {
            let ord = match self.group_sort_col {
                GroupSortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                GroupSortCol::StoredBytes => a.stored_bytes.cmp(&b.stored_bytes),
                GroupSortCol::Retention => a.retention_days.cmp(&b.retention_days),
                GroupSortCol::Created => a.created.cmp(&b.created),
            };
            if self.group_sort_dir == SortDir::Desc {
                ord.reverse()
            } else {
                ord
            }
        });
        result
    }

    fn filtered_streams(&self) -> Vec<&LogStreamInfo> {
        let mut result: Vec<&LogStreamInfo> = if self.filter.is_empty() {
            self.streams.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.streams
                .iter()
                .filter(|s| s.name.to_lowercase().contains(&f))
                .collect()
        };

        result.sort_by(|a, b| {
            let ord = match self.stream_sort_col {
                StreamSortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                StreamSortCol::LastEvent => a.last_event.cmp(&b.last_event),
                StreamSortCol::Created => a.created.cmp(&b.created),
            };
            if self.stream_sort_dir == SortDir::Desc {
                ord.reverse()
            } else {
                ord
            }
        });
        result
    }

    fn filtered_events(&self) -> Vec<&LogEvent> {
        if self.filter.is_empty() {
            self.events.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.events
                .iter()
                .filter(|e| e.message.to_lowercase().contains(&f))
                .collect()
        }
    }

    /// Auto-fetch next page when scrolled near the bottom
    fn maybe_auto_load(&mut self) -> Option<Action> {
        if self.loading || self.next_token.is_none() {
            return Some(Action::None);
        }
        let count = self.event_count();
        let near_bottom = self.event_scroll + self.event_visible_rows + 10 >= count;
        if near_bottom {
            self.loading = true;
            match self.events_source {
                EventsSource::Search => Some(Action::CwSearchNextPage),
                _ => Some(Action::CwNextPage),
            }
        } else {
            Some(Action::None)
        }
    }

    fn max_event_scroll(&self) -> usize {
        let count = self.event_count();
        count.saturating_sub(self.event_visible_rows)
    }

    /// Fast event count — avoids building a temp Vec
    fn event_count(&self) -> usize {
        if self.filter.is_empty() {
            self.events.len()
        } else {
            let f = self.filter.to_lowercase();
            self.events.iter().filter(|e| e.message.to_lowercase().contains(&f)).count()
        }
    }

    fn group_count(&self) -> usize {
        self.filtered_groups().len()
    }

    fn stream_count(&self) -> usize {
        self.filtered_streams().len()
    }
}

impl ServiceComponent for CloudWatchView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Popup handlers take priority
        if self.search_popup_open {
            return self.handle_search_popup_key(key);
        }
        if self.insights_popup_open {
            return self.handle_insights_popup_key(key);
        }

        // Events screen
        if self.screen == CwScreen::Events {
            if self.filtering {
                return self.handle_filter_key(key);
            }
            match key.code {
                KeyCode::Esc | KeyCode::Char('h') => {
                    self.pending_g = false;
                    self.go_back();
                    return Some(Action::None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.pending_g = false;
                    let max = self.max_event_scroll();
                    if self.event_scroll < max {
                        self.event_scroll += 1;
                    }
                    return self.maybe_auto_load();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.pending_g = false;
                    self.event_scroll = self.event_scroll.saturating_sub(1);
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.pending_g = false;
                    let max = self.max_event_scroll();
                    self.event_scroll = (self.event_scroll + 20).min(max);
                    return self.maybe_auto_load();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.pending_g = false;
                    self.event_scroll = self.event_scroll.saturating_sub(20);
                }
                KeyCode::Char('g') => {
                    if self.pending_g {
                        self.event_scroll = 0;
                        self.pending_g = false;
                    } else {
                        self.pending_g = true;
                    }
                }
                KeyCode::Char('G') => {
                    self.pending_g = false;
                    self.event_scroll = self.max_event_scroll();
                    return self.maybe_auto_load();
                }
                KeyCode::Char('/') => {
                    self.filtering = true;
                    self.filter.clear();
                }
                KeyCode::Char('r') => {
                    self.events.clear();
                    self.next_token = None;
                    self.event_scroll = 0;
                    self.loading = true;
                    self.error = None;
                    return Some(Action::Refresh);
                }
                _ => {
                    self.pending_g = false;
                }
            }
            return Some(Action::None);
        }

        // Filter mode for groups/streams
        if self.filtering {
            return self.handle_filter_key(key);
        }

        // Groups/Streams list navigation
        let count = match self.screen {
            CwScreen::Groups => self.group_count(),
            CwScreen::Streams => self.stream_count(),
            CwScreen::Events => 0,
        };
        let selected = match self.screen {
            CwScreen::Groups => &mut self.group_selected,
            CwScreen::Streams => &mut self.stream_selected,
            CwScreen::Events => return Some(Action::None),
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.pending_g = false;
                if count > 0 && *selected + 1 < count {
                    *selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    *selected = 0;
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
                return Some(Action::None);
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                if count > 0 {
                    *selected = count - 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.pending_g = false;
                if count > 0 {
                    *selected = (*selected + 20).min(count - 1);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.pending_g = false;
                *selected = selected.saturating_sub(20);
            }
            KeyCode::Char('s') => {
                match self.screen {
                    CwScreen::Groups => {
                        self.group_sort_col = match self.group_sort_col {
                            GroupSortCol::Name => GroupSortCol::StoredBytes,
                            GroupSortCol::StoredBytes => GroupSortCol::Retention,
                            GroupSortCol::Retention => GroupSortCol::Created,
                            GroupSortCol::Created => GroupSortCol::Name,
                        };
                        self.group_selected = 0;
                    }
                    CwScreen::Streams => {
                        self.stream_sort_col = match self.stream_sort_col {
                            StreamSortCol::Name => StreamSortCol::LastEvent,
                            StreamSortCol::LastEvent => StreamSortCol::Created,
                            StreamSortCol::Created => StreamSortCol::Name,
                        };
                        self.stream_selected = 0;
                    }
                    _ => {}
                }
            }
            KeyCode::Char('S') => {
                match self.screen {
                    CwScreen::Groups => {
                        self.group_sort_col = match self.group_sort_col {
                            GroupSortCol::Name => GroupSortCol::Created,
                            GroupSortCol::StoredBytes => GroupSortCol::Name,
                            GroupSortCol::Retention => GroupSortCol::StoredBytes,
                            GroupSortCol::Created => GroupSortCol::Retention,
                        };
                        self.group_selected = 0;
                    }
                    CwScreen::Streams => {
                        self.stream_sort_col = match self.stream_sort_col {
                            StreamSortCol::Name => StreamSortCol::Created,
                            StreamSortCol::LastEvent => StreamSortCol::Name,
                            StreamSortCol::Created => StreamSortCol::LastEvent,
                        };
                        self.stream_selected = 0;
                    }
                    _ => {}
                }
            }
            KeyCode::Char('x') => {
                match self.screen {
                    CwScreen::Groups => {
                        self.group_sort_dir = self.group_sort_dir.toggle();
                        self.group_selected = 0;
                    }
                    CwScreen::Streams => {
                        self.stream_sort_dir = self.stream_sort_dir.toggle();
                        self.stream_selected = 0;
                    }
                    _ => {}
                }
            }
            KeyCode::Char('r') => {
                self.loading = true;
                self.error = None;
                return Some(Action::Refresh);
            }
            KeyCode::Char('f') => {
                if self.screen == CwScreen::Groups {
                    if let Some(group) = self.selected_group().cloned() {
                        self.active_group = group.name;
                    }
                }
                if !self.active_group.is_empty() {
                    self.open_search_popup();
                }
            }
            KeyCode::Char(' ') => {
                if self.screen == CwScreen::Groups {
                    let group_name = self.filtered_groups()
                        .into_iter()
                        .nth(self.group_selected)
                        .map(|g| g.name.clone());
                    if let Some(name) = group_name {
                        if self.selected_groups.contains(&name) {
                            self.selected_groups.remove(&name);
                        } else {
                            self.selected_groups.insert(name);
                        }
                        // Move cursor down after toggling
                        if count > 0 && self.group_selected + 1 < count {
                            self.group_selected += 1;
                        }
                    }
                }
            }
            KeyCode::Char('i') => {
                if self.screen == CwScreen::Groups {
                    // Use selected groups if any, otherwise all filtered, capped at 50
                    let groups: Vec<String> = if !self.selected_groups.is_empty() {
                        self.selected_groups.iter().take(50).cloned().collect()
                    } else {
                        self.filtered_groups().iter().take(50).map(|g| g.name.clone()).collect()
                    };
                    if !groups.is_empty() {
                        self.insights_groups = groups;
                        self.open_insights_popup();
                    }
                } else if self.screen == CwScreen::Streams {
                    if !self.active_group.is_empty() {
                        self.insights_groups = vec![self.active_group.clone()];
                        self.open_insights_popup();
                    }
                }
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                self.filter.clear();
            }
            KeyCode::Enter => {
                return Some(Action::ServiceEnter);
            }
            KeyCode::Esc | KeyCode::Char('h') => {
                // Esc clears selection first, then goes back
                if !self.selected_groups.is_empty() && self.screen == CwScreen::Groups {
                    self.selected_groups.clear();
                    return Some(Action::None);
                }
                if self.go_back() {
                    return Some(Action::ServiceBack);
                } else {
                    return None;
                }
            }
            _ => {
                self.pending_g = false;
            }
        }
        Some(Action::None)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.refresh_flash > 0 {
            self.refresh_flash -= 1;
        }
        match self.screen {
            CwScreen::Groups => self.render_groups(frame, area),
            CwScreen::Streams => self.render_streams(frame, area),
            CwScreen::Events => self.render_events(frame, area),
        }
        if self.search_popup_open {
            self.render_search_popup(frame, area);
        }
        if self.insights_popup_open {
            self.render_insights_popup(frame, area);
        }
    }

    fn name(&self) -> &'static str {
        "CloudWatch"
    }
}

impl CloudWatchView {
    fn handle_filter_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
            }
            KeyCode::Enter => {
                self.filtering = false;
            }
            KeyCode::Backspace => {
                self.filter.pop();
                match self.screen {
                    CwScreen::Groups => self.group_selected = 0,
                    CwScreen::Streams => self.stream_selected = 0,
                    CwScreen::Events => self.event_scroll = 0,
                }
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                match self.screen {
                    CwScreen::Groups => self.group_selected = 0,
                    CwScreen::Streams => self.stream_selected = 0,
                    CwScreen::Events => self.event_scroll = 0,
                }
            }
            _ => {}
        }
        Some(Action::None)
    }

    fn render_groups(&mut self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading log groups...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, area);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            ));
            frame.render_widget(p, area);
            return;
        }

        let filtered = self.filtered_groups();
        let w = area.width as usize;
        let stored_col = 12;
        let retention_col = 12;
        let created_col = 12;
        let fixed = stored_col + retention_col + created_col + 8;
        let name_col = w.saturating_sub(fixed);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No log groups found"
            } else {
                "  No matching log groups"
            };
            let p = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)));
            frame.render_widget(p, area);
            if self.filtering {
                self.render_filter(frame, area);
            }
            return;
        }

        let header_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let active_hdr = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let col_hdr = |label: &str, col: GroupSortCol, width: usize| -> Span<'static> {
            let text = if self.group_sort_col == col {
                format!("{} {}", label, self.group_sort_dir.arrow())
            } else {
                label.to_string()
            };
            let style = if self.group_sort_col == col {
                active_hdr
            } else {
                header_style
            };
            Span::styled(format!("{:>width$}", text, width = width), style)
        };

        let name_hdr = if self.group_sort_col == GroupSortCol::Name {
            format!("Name {}", self.group_sort_dir.arrow())
        } else {
            "Name".to_string()
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", name_hdr, width = name_col),
                if self.group_sort_col == GroupSortCol::Name {
                    active_hdr
                } else {
                    header_style
                },
            ),
            col_hdr("Stored", GroupSortCol::StoredBytes, stored_col),
            col_hdr("Retention", GroupSortCol::Retention, retention_col),
            col_hdr("Created", GroupSortCol::Created, created_col),
            Span::styled("  ", header_style),
        ]));
        let header_area = Rect { height: 1, ..area };
        frame.render_widget(header, header_area);

        let data_area = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };

        let mut items: Vec<ListItem> = Vec::new();

        let has_selections = !self.selected_groups.is_empty();

        for (i, group) in filtered.iter().enumerate() {
            let is_cursor = i == self.group_selected;
            let is_checked = self.selected_groups.contains(&group.name);
            let style = if is_cursor {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if is_checked {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };

            let marker = if has_selections {
                if is_checked { "● " } else { "  " }
            } else {
                "  "
            };
            let marker_style = if is_checked {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                style
            };

            let stored = format_stored_bytes(group.stored_bytes);
            let retention = group
                .retention_days
                .map(|d| format!("{}d", d))
                .unwrap_or_else(|| "Never".to_string());
            let created = if group.created.len() >= 10 {
                &group.created[..10]
            } else {
                &group.created
            };
            let name_w = name_col.saturating_sub(if has_selections { 2 } else { 0 });
            let name = truncate_str(&group.name, name_w);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker.to_string(), marker_style),
                Span::styled(format!("{:<width$}", name, width = name_w), style),
                Span::styled(format!("{:>width$}", stored, width = stored_col), style),
                Span::styled(format!("{:>width$}", retention, width = retention_col), style),
                Span::styled(format!("{:>width$}", created, width = created_col), style),
                Span::styled("  ", style),
            ])));
        }

        let list = List::new(items);
        self.group_list_state.select(Some(self.group_selected));
        let needs_bottom_bar = self.filtering || has_selections;
        let render_area = if needs_bottom_bar {
            Rect { height: data_area.height.saturating_sub(1), ..data_area }
        } else {
            data_area
        };
        frame.render_stateful_widget(list, render_area, &mut self.group_list_state);

        if self.filtering {
            self.render_filter(frame, area);
        } else if !self.selected_groups.is_empty() {
            self.render_selection_bar(frame, area);
        } else if self.refresh_flash > 0 {
            self.render_refresh_flash(frame, area);
        }
    }

    fn render_selection_bar(&self, frame: &mut Frame, area: Rect) {
        let bar_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        let bar_style = Style::default().fg(Color::Black).bg(Color::Cyan);
        let mut text = format!(
            " {} selected | Space toggle | i insights | Esc clear",
            self.selected_groups.len()
        );
        let pad = (bar_area.width as usize).saturating_sub(text.len());
        text.push_str(&" ".repeat(pad));
        let p = Paragraph::new(Span::styled(text, bar_style));
        frame.render_widget(p, bar_area);
    }

    fn render_streams(&mut self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading log streams...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, area);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            ));
            frame.render_widget(p, area);
            return;
        }

        let filtered = self.filtered_streams();
        let w = area.width as usize;
        let last_event_col = 20;
        let created_col = 12;
        let fixed = last_event_col + created_col + 8;
        let name_col = w.saturating_sub(fixed);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No log streams found"
            } else {
                "  No matching log streams"
            };
            let p = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)));
            frame.render_widget(p, area);
            if self.filtering {
                self.render_filter(frame, area);
            }
            return;
        }

        let header_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let active_hdr = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let col_hdr = |label: &str, col: StreamSortCol, width: usize| -> Span<'static> {
            let text = if self.stream_sort_col == col {
                format!("{} {}", label, self.stream_sort_dir.arrow())
            } else {
                label.to_string()
            };
            let style = if self.stream_sort_col == col {
                active_hdr
            } else {
                header_style
            };
            Span::styled(format!("{:>width$}", text, width = width), style)
        };

        let name_hdr = if self.stream_sort_col == StreamSortCol::Name {
            format!("Name {}", self.stream_sort_dir.arrow())
        } else {
            "Name".to_string()
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", name_hdr, width = name_col),
                if self.stream_sort_col == StreamSortCol::Name {
                    active_hdr
                } else {
                    header_style
                },
            ),
            col_hdr("Last Event", StreamSortCol::LastEvent, last_event_col),
            col_hdr("Created", StreamSortCol::Created, created_col),
            Span::styled("  ", header_style),
        ]));
        let header_area = Rect { height: 1, ..area };
        frame.render_widget(header, header_area);

        let data_area = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };

        let mut items: Vec<ListItem> = Vec::new();

        for (i, stream) in filtered.iter().enumerate() {
            let is_selected = i == self.stream_selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let last_event = stream
                .last_event
                .as_deref()
                .unwrap_or("-");
            let created = if stream.created.len() >= 10 {
                &stream.created[..10]
            } else {
                &stream.created
            };
            let name = truncate_str(&stream.name, name_col);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(
                    format!("{:>width$}", last_event, width = last_event_col),
                    style,
                ),
                Span::styled(format!("{:>width$}", created, width = created_col), style),
                Span::styled("  ", style),
            ])));
        }

        let list = List::new(items);
        self.stream_list_state.select(Some(self.stream_selected));
        let render_area = if self.filtering {
            Rect { height: data_area.height.saturating_sub(1), ..data_area }
        } else {
            data_area
        };
        frame.render_stateful_widget(list, render_area, &mut self.stream_list_state);

        if self.filtering {
            self.render_filter(frame, area);
        } else if self.refresh_flash > 0 {
            self.render_refresh_flash(frame, area);
        }
    }

    fn render_events(&mut self, frame: &mut Frame, area: Rect) {
        if area.height < 2 {
            return;
        }

        // Reserve bottom row for info bar
        let content_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.saturating_sub(1),
        };
        let info_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };

        if self.loading && self.events.is_empty() {
            let p = Paragraph::new(Span::styled(
                "  Loading log events...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, content_area);
            self.render_event_info_bar_with(frame, info_area, 0, 0);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("  Error: {} (r to retry)", err),
                Style::default().fg(Color::Red),
            ));
            frame.render_widget(p, content_area);
            self.render_event_info_bar_with(frame, info_area, 0, self.events.len());
            return;
        }

        // When no filter active, skip building a temporary Vec — iterate self.events directly.
        // This matters at 2k+ events.
        let (event_count, total_count) = if self.filter.is_empty() {
            let n = self.events.len();
            (n, n)
        } else {
            let f = self.filter.to_lowercase();
            let n = self.events.iter().filter(|e| e.message.to_lowercase().contains(&f)).count();
            (n, self.events.len())
        };

        if event_count == 0 {
            let msg = if self.filter.is_empty() {
                "  No log events found"
            } else {
                "  No matching log events"
            };
            let p = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)));
            frame.render_widget(p, content_area);
            self.render_event_info_bar_with(frame, info_area, event_count, total_count);
            if self.filtering {
                self.render_filter(frame, content_area);
            }
            return;
        }

        let visible_rows = content_area.height as usize;
        self.event_visible_rows = visible_rows;
        let ts_col = 20;

        // Clamp scroll so we can't scroll past the last screenful
        let max_scroll = event_count.saturating_sub(visible_rows);
        if self.event_scroll > max_scroll {
            self.event_scroll = max_scroll;
        }

        let start = self.event_scroll;
        let end = (start + visible_rows).min(event_count);

        let sep_style = Style::default().fg(Color::Rgb(60, 60, 60));
        let ts_style = Style::default().fg(Color::DarkGray);
        let msg_style = Style::default().fg(Color::Gray);
        let msg_width = (area.width as usize).saturating_sub(ts_col + 7);

        let mut items: Vec<ListItem> = Vec::new();

        if self.filter.is_empty() {
            // Fast path — no filter, index directly
            for event in &self.events[start..end] {
                let ts = format_epoch_ms(event.timestamp);
                let msg = parse_log_message(&event.message);
                let display_msg = truncate_str(&msg, msg_width);
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<width$} ", ts, width = ts_col), ts_style),
                    Span::styled("│ ", sep_style),
                    Span::styled(display_msg, msg_style),
                ])));
            }
        } else {
            // Filtered path — skip + take only visible range
            let f = self.filter.to_lowercase();
            for event in self.events.iter()
                .filter(|e| e.message.to_lowercase().contains(&f))
                .skip(start)
                .take(end - start)
            {
                let ts = format_epoch_ms(event.timestamp);
                let msg = parse_log_message(&event.message);
                let display_msg = truncate_str(&msg, msg_width);
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("  {:<width$} ", ts, width = ts_col), ts_style),
                    Span::styled("│ ", sep_style),
                    Span::styled(display_msg, msg_style),
                ])));
            }
        }

        let list = List::new(items);
        frame.render_widget(list, content_area);

        self.render_event_info_bar_with(frame, info_area, event_count, total_count);

        if self.filtering {
            self.render_filter(frame, content_area);
        }
    }

    fn render_event_info_bar_with(&self, frame: &mut Frame, area: Rect, event_count: usize, total_count: usize) {
        let info_style = Style::default().fg(Color::Black).bg(Color::Rgb(204, 120, 50));

        let mut info_text = if self.refresh_flash > 0 {
            " ✓ Refreshed".to_string()
        } else {
            format!(" {} events", event_count)
        };
        if self.refresh_flash == 0 {
            if event_count != total_count {
                info_text.push_str(&format!(" (of {} loaded)", total_count));
            }
            if self.loading {
                info_text.push_str(" | loading...");
            }
            match self.events_source {
                EventsSource::Search => {
                    info_text.push_str(&format!(" | pattern: \"{}\"", &self.search_pattern_display));
                }
                EventsSource::Insights => {
                    if let Some(ref status) = self.insights_status {
                        info_text.push_str(&format!(" | {}", status));
                    }
                }
                EventsSource::Stream => {
                    info_text.push_str(" | r: refresh");
                }
            }
        }

        let pad = (area.width as usize).saturating_sub(info_text.len());
        info_text.push_str(&" ".repeat(pad));

        let p = Paragraph::new(Span::styled(info_text, info_style));
        frame.render_widget(p, area);
    }

    fn render_refresh_flash(&self, frame: &mut Frame, area: Rect) {
        let flash_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        let flash_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(204, 120, 50));
        let mut text = " ✓ Refreshed".to_string();
        let pad = (flash_area.width as usize).saturating_sub(text.len());
        text.push_str(&" ".repeat(pad));
        let p = Paragraph::new(Span::styled(text, flash_style));
        frame.render_widget(p, flash_area);
    }

    fn render_filter(&self, frame: &mut Frame, area: Rect) {
        let filter_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        let clear = Paragraph::new(Span::styled(
            " ".repeat(filter_area.width as usize),
            Style::default(),
        ));
        frame.render_widget(clear, filter_area);

        let p = Paragraph::new(Line::from(vec![
            Span::styled(" search: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&self.filter, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::Gray)),
        ]));
        frame.render_widget(p, filter_area);
    }

    fn handle_search_popup_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Editing pattern text
        if self.search_editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.search_editing = false,
                KeyCode::Backspace => { self.search_pattern.pop(); }
                KeyCode::Char(c) => self.search_pattern.push(c),
                _ => {}
            }
            return Some(Action::None);
        }

        // Editing hours text
        if self.search_hours_editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.search_hours_editing = false,
                KeyCode::Backspace => { self.search_hours.pop(); }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => self.search_hours.push(c),
                _ => {}
            }
            return Some(Action::None);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.search_field = match self.search_field {
                    SearchField::Pattern => SearchField::TimeRange,
                    SearchField::TimeRange => SearchField::Pattern,
                };
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.search_field = match self.search_field {
                    SearchField::Pattern => SearchField::TimeRange,
                    SearchField::TimeRange => SearchField::Pattern,
                };
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                match self.search_field {
                    SearchField::Pattern => self.search_editing = true,
                    SearchField::TimeRange => self.search_hours_editing = true,
                }
            }
            KeyCode::Char('r') => {
                if !self.search_pattern.is_empty() {
                    self.loading = true;
                    return Some(Action::CwRunSearch);
                }
            }
            KeyCode::Esc => {
                self.search_popup_open = false;
            }
            _ => {}
        }
        Some(Action::None)
    }

    fn handle_insights_popup_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Editing query text (multi-line)
        if self.insights_editing {
            match key.code {
                KeyCode::Esc => self.insights_editing = false,
                KeyCode::Enter => self.insights_query.push('\n'),
                KeyCode::Backspace => { self.insights_query.pop(); }
                KeyCode::Char(c) => self.insights_query.push(c),
                _ => {}
            }
            return Some(Action::None);
        }

        // Editing hours text
        if self.insights_hours_editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.insights_hours_editing = false,
                KeyCode::Backspace => { self.insights_hours.pop(); }
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => self.insights_hours.push(c),
                _ => {}
            }
            return Some(Action::None);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.insights_field = match self.insights_field {
                    InsightsField::Query => InsightsField::TimeRange,
                    InsightsField::TimeRange => InsightsField::Query,
                };
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.insights_field = match self.insights_field {
                    InsightsField::Query => InsightsField::TimeRange,
                    InsightsField::TimeRange => InsightsField::Query,
                };
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                match self.insights_field {
                    InsightsField::Query => self.insights_editing = true,
                    InsightsField::TimeRange => self.insights_hours_editing = true,
                }
            }
            KeyCode::Char('r') => {
                if !self.insights_query.trim().is_empty() {
                    self.loading = true;
                    self.insights_status = Some("Starting...".to_string());
                    return Some(Action::CwRunInsights);
                }
            }
            KeyCode::Esc => {
                self.insights_popup_open = false;
            }
            _ => {}
        }
        Some(Action::None)
    }

    fn render_search_popup(&self, frame: &mut Frame, area: Rect) {
        let popup_width: u16 = 60u16.min(area.width.saturating_sub(4));
        // 2 fields + blank + footer + 2 borders, +1 if status
        let content: u16 = 4 + if self.search_status.is_some() { 1 } else { 0 } + 2;
        let popup_height: u16 = content.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect { x, y, width: popup_width, height: popup_height };
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Search Log Group ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 100)))
            .style(Style::default().bg(Color::Rgb(30, 30, 40)));

        let sel = Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD);
        let label_s = Style::default().fg(Color::DarkGray);
        let val_s = Style::default().fg(Color::Cyan);
        let edit_s = Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80));
        let footer_s = Style::default().fg(Color::DarkGray);

        let iw = popup_width.saturating_sub(2) as usize;

        let pattern_display = if self.search_editing {
            format!("{}█", &self.search_pattern)
        } else if self.search_pattern.is_empty() {
            "(enter pattern)".to_string()
        } else {
            self.search_pattern.clone()
        };

        let pattern_style = if self.search_editing {
            edit_s
        } else if self.search_field == SearchField::Pattern {
            sel
        } else if self.search_pattern.is_empty() {
            Style::default().fg(Color::Rgb(80, 80, 80))
        } else {
            val_s
        };

        let hours_display = if self.search_hours_editing {
            format!("{}█", &self.search_hours)
        } else if self.search_hours.is_empty() {
            "3".to_string()
        } else {
            self.search_hours.clone()
        };
        let hours_label = format!("{} hours", hours_display);

        let time_style = if self.search_hours_editing {
            edit_s
        } else if self.search_field == SearchField::TimeRange {
            sel
        } else {
            val_s
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(" Pattern:  ", label_s),
                Span::styled(
                    format!("{:<w$}", pattern_display, w = iw.saturating_sub(11)),
                    pattern_style,
                ),
            ]),
            Line::from(vec![
                Span::styled(" Hours:    ", label_s),
                Span::styled(
                    format!("{:<w$}", hours_label, w = iw.saturating_sub(11)),
                    time_style,
                ),
            ]),
        ];

        if let Some(ref status) = self.search_status {
            lines.push(Line::from(Span::styled(
                format!(" {}", status),
                Style::default().fg(Color::Yellow),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" r", Style::default().fg(Color::Yellow)),
            Span::styled(" run  ", footer_s),
            Span::styled("i", Style::default().fg(Color::Yellow)),
            Span::styled(" edit  ", footer_s),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(" cancel", footer_s),
        ]));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }

    fn render_insights_popup(&self, frame: &mut Frame, area: Rect) {
        let popup_width: u16 = 70u16.min(area.width.saturating_sub(4));
        // groups + "Query:" + query lines (min 3) + blank + hours + status? + blank + footer + 2 borders
        let query_lines = self.insights_query.split('\n').count().max(3) as u16;
        let content: u16 = 1 + 1 + query_lines + 1 + 1
            + if self.insights_status.is_some() { 1 } else { 0 }
            + 1 + 1 + 2;
        let popup_height: u16 = content.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect { x, y, width: popup_width, height: popup_height };
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Logs Insights ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 100)))
            .style(Style::default().bg(Color::Rgb(30, 30, 40)));

        let sel = Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD);
        let label_s = Style::default().fg(Color::DarkGray);
        let val_s = Style::default().fg(Color::Cyan);
        let edit_s = Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80));
        let footer_s = Style::default().fg(Color::DarkGray);

        let iw = popup_width.saturating_sub(2) as usize;

        let mut lines: Vec<Line> = Vec::new();

        // Show which groups are being queried
        let groups_label = if self.insights_groups.len() == 1 {
            let name = shorten_group_name(&self.insights_groups[0]);
            format!("{}", name)
        } else if self.insights_groups.len() >= 50 {
            format!("{} groups (max 50 — Space to select)", self.insights_groups.len())
        } else {
            format!("{} groups (selected)", self.insights_groups.len())
        };
        lines.push(Line::from(vec![
            Span::styled(" Groups:     ", label_s),
            Span::styled(
                format!("{:<w$}", groups_label, w = iw.saturating_sub(13)),
                Style::default().fg(Color::Cyan),
            ),
        ]));

        lines.push(Line::from(Span::styled(" Query:", label_s)));

        let query_lines: Vec<&str> = self.insights_query.split('\n').collect();
        let query_style = if self.insights_editing {
            edit_s
        } else if self.insights_field == InsightsField::Query {
            sel
        } else {
            val_s
        };

        for (i, ql) in query_lines.iter().enumerate() {
            let display = if self.insights_editing && i == query_lines.len() - 1 {
                format!(" {}█", ql)
            } else {
                format!(" {}", ql)
            };
            lines.push(Line::from(Span::styled(
                format!("{:<w$}", display, w = iw),
                query_style,
            )));
        }

        lines.push(Line::from(""));

        let hours_display = if self.insights_hours_editing {
            format!("{}█", &self.insights_hours)
        } else if self.insights_hours.is_empty() {
            "3".to_string()
        } else {
            self.insights_hours.clone()
        };
        let hours_label = format!("{} hours", hours_display);

        let time_style = if self.insights_hours_editing {
            edit_s
        } else if self.insights_field == InsightsField::TimeRange {
            sel
        } else {
            val_s
        };
        lines.push(Line::from(vec![
            Span::styled(" Hours:      ", label_s),
            Span::styled(
                format!("{:<w$}", hours_label, w = iw.saturating_sub(13)),
                time_style,
            ),
        ]));

        if let Some(ref status) = self.insights_status {
            lines.push(Line::from(Span::styled(
                format!(" Status: {}", status),
                Style::default().fg(Color::Yellow),
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" r", Style::default().fg(Color::Yellow)),
            Span::styled(" run  ", footer_s),
            Span::styled("i", Style::default().fg(Color::Yellow)),
            Span::styled(" edit  ", footer_s),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(" cancel", footer_s),
        ]));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}

/// Parse Lambda CloudWatch log messages to extract the human-readable part.
/// Lambda log format: `<ISO_TIMESTAMP>\t<REQUEST_ID>\t<LEVEL>\t<MESSAGE>\n`
/// or the older format: `<ISO_TIMESTAMP><REQUEST_ID><LEVEL><MESSAGE>`
/// Also handles START/END/REPORT lines and plain messages.
fn parse_log_message(raw: &str) -> String {
    let msg = raw.trim();

    // START/END/REPORT lines are already clean
    if msg.starts_with("START ") || msg.starts_with("END ") || msg.starts_with("REPORT ") {
        return msg.to_string();
    }

    // Tab-delimited format (newer Lambda runtime):
    // 2026-03-15T21:48:30.486Z\tREQUEST_ID\tINFO\tactual message
    if msg.contains('\t') {
        let parts: Vec<&str> = msg.splitn(4, '\t').collect();
        if parts.len() >= 4 {
            return format!("{} {}", parts[2], parts[3].trim());
        }
        if parts.len() == 3 {
            return format!("{} {}", parts[1], parts[2].trim());
        }
    }

    // Older no-delimiter format:
    // 2026-03-15T21:48:30.486Z<UUID>INFO|DEBUG|WARN|ERROR<message>
    // Try to find the log level keyword after the UUID pattern
    let log_levels = ["INFO", "DEBUG", "WARN", "ERROR", "CRITICAL"];
    // Skip past the ISO timestamp (at least 24 chars like 2026-03-15T21:48:30.486Z)
    if msg.len() > 24 && msg.as_bytes().get(4) == Some(&b'-') {
        let after_ts = &msg[24..]; // skip "2026-03-15T21:48:30.486Z"
        // Look for a log level keyword in the next ~40 chars (UUID is 36 chars)
        let search_range = after_ts.len().min(50);
        let search_slice = &after_ts[..search_range];
        for level in &log_levels {
            if let Some(pos) = search_slice.find(level) {
                let after_level = &after_ts[pos + level.len()..];
                return format!("{} {}", level, after_level.trim());
            }
        }
    }

    // Fallback: return as-is
    msg.to_string()
}

fn shorten_group_name(name: &str) -> &str {
    let last_slash = name.rfind('/').unwrap_or(0);
    let second_last = name[..last_slash].rfind('/').unwrap_or(0);
    &name[second_last..]
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_string()
    }
}
