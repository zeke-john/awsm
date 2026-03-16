use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::Action;
use crate::aws::s3::{BucketInfo, ObjectDetail, ObjectInfo, format_size};
use crate::ui::services::ServiceComponent;

#[derive(Debug, Clone, PartialEq, Eq)]
enum S3Screen {
    Buckets,
    Objects,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
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
pub struct S3View {
    screen: S3Screen,
    buckets: Vec<BucketInfo>,
    objects: Vec<ObjectInfo>,
    pub detail: Option<ObjectDetail>,
    pub detail_scroll: u16,
    selected: usize,
    list_state: ListState,
    bucket_name: String,
    prefix: String,
    prefix_stack: Vec<String>,
    loading: bool,
    error: Option<String>,
    filter: String,
    filtering: bool,
    pub status_msg: Option<String>,
    pending_g: bool,
    detail_total_lines: u16,
    sort_column: SortColumn,
    sort_dir: SortDir,
}

impl Default for S3View {
    fn default() -> Self {
        Self {
            screen: S3Screen::Buckets,
            buckets: Vec::new(),
            objects: Vec::new(),
            detail: None,
            detail_scroll: 0,
            selected: 0,
            list_state: ListState::default(),
            bucket_name: String::new(),
            prefix: String::new(),
            prefix_stack: Vec::new(),
            loading: true,
            error: None,
            filter: String::new(),
            filtering: false,
            status_msg: None,
            pending_g: false,
            detail_total_lines: 0,
            sort_column: SortColumn::Name,
            sort_dir: SortDir::Asc,
        }
    }
}

impl S3View {
    pub fn set_buckets(&mut self, buckets: Vec<BucketInfo>) {
        self.buckets = buckets;
        self.loading = false;
        self.error = None;
        self.selected = 0;
        self.list_state = ListState::default();
    }

    pub fn set_objects(&mut self, objects: Vec<ObjectInfo>) {
        self.objects = objects;
        self.loading = false;
        self.error = None;
        self.selected = 0;
        self.list_state = ListState::default();
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    pub fn set_loading(&mut self) {
        self.loading = true;
        self.error = None;
    }

    pub fn enter_bucket(&mut self, bucket: String) {
        self.bucket_name = bucket;
        self.prefix = String::new();
        self.prefix_stack.clear();
        self.screen = S3Screen::Objects;
        self.selected = 0;
        self.list_state = ListState::default();
        self.loading = true;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn enter_detail(&mut self) {
        self.screen = S3Screen::Detail;
        self.detail = None;
        self.detail_scroll = 0;
        self.loading = true;
        self.status_msg = None;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn set_detail(&mut self, detail: ObjectDetail) {
        self.detail = Some(detail);
        self.loading = false;
        self.error = None;
    }

    pub fn enter_prefix(&mut self, prefix: String) {
        self.prefix_stack.push(self.prefix.clone());
        self.prefix = prefix;
        self.selected = 0;
        self.list_state = ListState::default();
        self.loading = true;
        self.filter.clear();
        self.filtering = false;
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
            S3Screen::Detail => {
                self.screen = S3Screen::Objects;
                self.detail = None;
                self.loading = false;
                true
            }
            S3Screen::Objects => {
                if let Some(prev) = self.prefix_stack.pop() {
                    self.prefix = prev;
                    self.selected = 0;
                    self.loading = true;
                    true
                } else {
                    self.screen = S3Screen::Buckets;
                    self.selected = 0;
                    self.loading = false;
                    true
                }
            }
            S3Screen::Buckets => false,
        }
    }

    pub fn selected_bucket(&self) -> Option<&BucketInfo> {
        let filtered = self.filtered_buckets();
        filtered.into_iter().nth(self.selected)
    }

    pub fn selected_object(&self) -> Option<&ObjectInfo> {
        let filtered = self.filtered_objects();
        filtered.into_iter().nth(self.selected)
    }

    pub fn is_editing(&self) -> bool {
        self.filtering
    }

    pub fn needs_bucket_load(&self) -> bool {
        self.screen == S3Screen::Buckets && self.loading
    }

    pub fn needs_object_load(&self) -> bool {
        self.screen == S3Screen::Objects && self.loading
    }

    pub fn current_bucket(&self) -> &str {
        &self.bucket_name
    }

    pub fn current_prefix(&self) -> &str {
        &self.prefix
    }

    fn filtered_buckets(&self) -> Vec<&BucketInfo> {
        let mut result: Vec<&BucketInfo> = if self.filter.is_empty() {
            self.buckets.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.buckets
                .iter()
                .filter(|b| b.name.to_lowercase().contains(&f))
                .collect()
        };

        match self.sort_column {
            SortColumn::Name => result.sort_by(|a, b| a.name.cmp(&b.name)),
            SortColumn::Modified | SortColumn::Size => {
                result.sort_by(|a, b| a.created.cmp(&b.created));
            }
        }
        if self.sort_dir == SortDir::Desc {
            result.reverse();
        }
        result
    }

    fn filtered_objects(&self) -> Vec<&ObjectInfo> {
        let mut result: Vec<&ObjectInfo> = if self.filter.is_empty() {
            self.objects.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.objects
                .iter()
                .filter(|o| o.display_name.to_lowercase().contains(&f))
                .collect()
        };

        // Folders always first
        result.sort_by(|a, b| {
            match (a.is_prefix, b.is_prefix) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let ord = match self.sort_column {
                        SortColumn::Name => a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()),
                        SortColumn::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
                        SortColumn::Modified => a.last_modified.cmp(&b.last_modified),
                    };
                    if self.sort_dir == SortDir::Desc { ord.reverse() } else { ord }
                }
            }
        });
        result
    }

    fn item_count(&self) -> usize {
        match self.screen {
            S3Screen::Buckets => self.filtered_buckets().len(),
            S3Screen::Objects => self.filtered_objects().len(),
            S3Screen::Detail => 0,
        }
    }

    pub fn screen_type(&self) -> &str {
        match self.screen {
            S3Screen::Buckets => "buckets",
            S3Screen::Objects => "objects",
            S3Screen::Detail => "detail",
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.screen {
            S3Screen::Buckets => "S3 > Buckets".to_string(),
            S3Screen::Objects => {
                if self.prefix.is_empty() {
                    format!("S3 > {}", self.bucket_name)
                } else {
                    format!("S3 > {} > {}", self.bucket_name, self.prefix)
                }
            }
            S3Screen::Detail => {
                let key = self
                    .detail
                    .as_ref()
                    .map(|d| d.key.as_str())
                    .unwrap_or("...");
                let name = key.rsplit('/').next().unwrap_or(key);
                format!("S3 > {} > {}", self.bucket_name, name)
            }
        }
    }
}

impl ServiceComponent for S3View {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.screen == S3Screen::Detail {
            match key.code {
                KeyCode::Esc | KeyCode::Char('h') => {
                    self.pending_g = false;
                    self.go_back();
                    return Some(Action::None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                }
                KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_scroll.saturating_add(20);
                }
                KeyCode::Char('u') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_scroll.saturating_sub(20);
                }
                KeyCode::Char('g') => {
                    if self.pending_g {
                        self.detail_scroll = 0;
                        self.pending_g = false;
                    } else {
                        self.pending_g = true;
                    }
                }
                KeyCode::Char('G') => {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_total_lines;
                }
                KeyCode::Char('d') => {
                    self.pending_g = false;
                    return Some(Action::S3Download);
                }
                _ => {
                    self.pending_g = false;
                }
            }
            return Some(Action::None);
        }

        if self.filtering {
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
                    self.selected = 0;
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.selected = 0;
                }
                _ => {}
            }
            return Some(Action::None);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let count = self.item_count();
                if count > 0 && self.selected + 1 < count {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    self.selected = 0;
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
                return Some(Action::None);
            }
            KeyCode::Char('G') => {
                self.pending_g = false;
                let count = self.item_count();
                if count > 0 {
                    self.selected = count - 1;
                }
            }
            KeyCode::Char('s') => {
                self.sort_column = match (&self.screen, &self.sort_column) {
                    (S3Screen::Buckets, SortColumn::Name) => SortColumn::Modified,
                    (S3Screen::Buckets, _) => SortColumn::Name,
                    (S3Screen::Objects, SortColumn::Name) => SortColumn::Size,
                    (S3Screen::Objects, SortColumn::Size) => SortColumn::Modified,
                    (S3Screen::Objects, SortColumn::Modified) => SortColumn::Name,
                    _ => SortColumn::Name,
                };
                self.selected = 0;
            }
            KeyCode::Char('S') => {
                self.sort_column = match (&self.screen, &self.sort_column) {
                    (S3Screen::Buckets, SortColumn::Name) => SortColumn::Modified,
                    (S3Screen::Buckets, _) => SortColumn::Name,
                    (S3Screen::Objects, SortColumn::Name) => SortColumn::Modified,
                    (S3Screen::Objects, SortColumn::Modified) => SortColumn::Size,
                    (S3Screen::Objects, SortColumn::Size) => SortColumn::Name,
                    _ => SortColumn::Name,
                };
                self.selected = 0;
            }
            KeyCode::Char('x') => {
                self.sort_dir = self.sort_dir.toggle();
                self.selected = 0;
            }
            KeyCode::Char('r') => {
                if self.error.is_some() {
                    self.loading = true;
                    self.error = None;
                    return Some(Action::ServiceBack);
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
                if self.go_back() {
                    return Some(Action::ServiceBack);
                } else {
                    return None;
                }
            }
            _ => {}
        }
        Some(Action::None)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        match self.screen {
            S3Screen::Buckets => self.render_buckets(frame, area),
            S3Screen::Objects => self.render_objects(frame, area),
            S3Screen::Detail => self.render_detail(frame, area),
        }
    }

    fn name(&self) -> &'static str {
        "S3"
    }
}

impl S3View {
    fn render_buckets(&mut self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading buckets...",
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

        let filtered = self.filtered_buckets();

        let w = area.width as usize;
        let date_col = 12;
        let name_col = w.saturating_sub(date_col + 4);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No buckets found"
            } else {
                "  No matching buckets"
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

        let name_hdr = if self.sort_column == SortColumn::Name {
            format!("Name {}", self.sort_dir.arrow())
        } else {
            "Name".to_string()
        };
        let date_hdr = if self.sort_column == SortColumn::Modified {
            format!("Created {}", self.sort_dir.arrow())
        } else {
            "Created".to_string()
        };
        let active_hdr = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", name_hdr, width = name_col),
                if self.sort_column == SortColumn::Name { active_hdr } else { header_style },
            ),
            Span::styled(
                format!("{:>width$}", date_hdr, width = date_col),
                if self.sort_column == SortColumn::Modified { active_hdr } else { header_style },
            ),
        ]));
        let header_area = Rect { height: 1, ..area };
        frame.render_widget(header, header_area);

        let data_area = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };

        let mut items: Vec<ListItem> = Vec::new();

        for (i, bucket) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let created = bucket
                .created
                .as_deref()
                .and_then(|c| c.get(..10))
                .unwrap_or("-");

            let name = truncate_str(&bucket.name, name_col);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(format!("{:>width$}", created, width = date_col), style),
            ])));
        }

        let list = List::new(items);
        self.list_state.select(Some(self.selected));
        let render_area = if self.filtering {
            Rect { height: data_area.height.saturating_sub(1), ..data_area }
        } else {
            data_area
        };
        frame.render_stateful_widget(list, render_area, &mut self.list_state);

        if self.filtering {
            self.render_filter(frame, area);
        }
    }

    fn render_objects(&mut self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading...",
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

        let filtered = self.filtered_objects();

        let w = area.width as usize;
        let date_col = 12;
        let size_col = 10;
        let name_col = w.saturating_sub(date_col + size_col + 6);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  Empty"
            } else {
                "  No matching objects"
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

        let name_hdr = if self.sort_column == SortColumn::Name {
            format!("Name {}", self.sort_dir.arrow())
        } else {
            "Name".to_string()
        };
        let size_hdr = if self.sort_column == SortColumn::Size {
            format!("Size {}", self.sort_dir.arrow())
        } else {
            "Size".to_string()
        };
        let mod_hdr = if self.sort_column == SortColumn::Modified {
            format!("Modified {}", self.sort_dir.arrow())
        } else {
            "Modified".to_string()
        };
        let active_hdr = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", name_hdr, width = name_col),
                if self.sort_column == SortColumn::Name { active_hdr } else { header_style },
            ),
            Span::styled(
                format!("{:>width$}", size_hdr, width = size_col),
                if self.sort_column == SortColumn::Size { active_hdr } else { header_style },
            ),
            Span::styled(
                format!("{:>width$}  ", mod_hdr, width = date_col),
                if self.sort_column == SortColumn::Modified { active_hdr } else { header_style },
            ),
        ]));
        let header_area = Rect { height: 1, ..area };
        frame.render_widget(header, header_area);

        let data_area = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };

        let mut items: Vec<ListItem> = Vec::new();

        for (i, obj) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if obj.is_prefix {
                Style::default().fg(Color::Blue)
            } else {
                Style::default().fg(Color::Gray)
            };

            let size_str = obj
                .size
                .map(|s| format_size(s))
                .unwrap_or_else(|| "-".to_string());
            let modified = obj
                .last_modified
                .as_deref()
                .and_then(|m| m.get(..10))
                .unwrap_or("-");

            let name = truncate_str(&obj.display_name, name_col);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(format!("{:>width$}", size_str, width = size_col), style),
                Span::styled(format!("{:>width$}  ", modified, width = date_col), style),
            ])));
        }

        let list = List::new(items);
        self.list_state.select(Some(self.selected));
        let render_area = if self.filtering {
            Rect { height: data_area.height.saturating_sub(1), ..data_area }
        } else {
            data_area
        };
        frame.render_stateful_widget(list, render_area, &mut self.list_state);

        if self.filtering {
            self.render_filter(frame, area);
        }
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect) {
        let pad = 3;
        let inner = Rect {
            x: area.x + pad,
            y: area.y,
            width: area.width.saturating_sub(pad * 2),
            height: area.height,
        };

        if inner.width < 10 || inner.height < 3 {
            return;
        }

        if self.loading {
            let p = Paragraph::new(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, inner);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("Error: {}", err),
                Style::default().fg(Color::Red),
            ))
            .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(p, inner);
            return;
        }

        let detail = match &self.detail {
            Some(d) => d,
            None => return,
        };

        let label_style = Style::default().fg(Color::Yellow);
        let value_style = Style::default().fg(Color::White);
        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let size_str = format_size(detail.size);
        let uri = crate::aws::s3::s3_uri(&self.bucket_name, &detail.key);
        let arn = format!("arn:aws:s3:::{}/{}", self.bucket_name, detail.key);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled("Metadata", header_style)),
            Line::from(""),
            detail_line("Key", &detail.key, label_style, value_style),
            detail_line("Size", &size_str, label_style, value_style),
            detail_line("Content-Type", &detail.content_type, label_style, value_style),
            detail_line("Last Modified", &detail.last_modified, label_style, value_style),
            detail_line("Storage Class", &detail.storage_class, label_style, value_style),
            detail_line("ETag", &detail.etag, label_style, value_style),
            detail_line("S3 URI", &uri, label_style, Style::default().fg(Color::Blue)),
            detail_line("ARN", &arn, label_style, Style::default().fg(Color::Blue)),
        ];

        if let Some(ref vid) = detail.version_id {
            lines.push(detail_line("Version ID", vid, label_style, value_style));
        }
        if let Some(ref sse) = detail.server_side_encryption {
            lines.push(detail_line("Encryption", sse, label_style, value_style));
        }
        if let Some(ref kms) = detail.sse_kms_key_id {
            lines.push(detail_line("KMS Key ID", kms, label_style, value_style));
        }
        if let Some(ref enc) = detail.content_encoding {
            lines.push(detail_line("Encoding", enc, label_style, value_style));
        }
        if let Some(ref lang) = detail.content_language {
            lines.push(detail_line("Language", lang, label_style, value_style));
        }
        if let Some(ref cc) = detail.cache_control {
            lines.push(detail_line("Cache-Control", cc, label_style, value_style));
        }
        if let Some(ref cd) = detail.content_disposition {
            lines.push(detail_line("Disposition", cd, label_style, value_style));
        }

        if !detail.metadata.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("User Metadata", header_style)));
            lines.push(Line::from(""));
            for (k, v) in &detail.metadata {
                lines.push(detail_line(k, v, label_style, value_style));
            }
        }

        lines.push(Line::from(""));

        let dl_key = Style::default()
            .fg(Color::Rgb(255, 165, 0))
            .add_modifier(Modifier::BOLD);
        let dl_text = Style::default().fg(Color::Rgb(255, 165, 0));
        lines.push(Line::from(vec![
            Span::styled("[d]", dl_key),
            Span::styled(" Download file to ~/Downloads", dl_text),
        ]));

        if let Some(ref msg) = self.status_msg {
            let msg_style = if msg.starts_with("Error") {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(Span::styled(format!("    {}", msg), msg_style)));
        }

        if let Some(ref content) = detail.content {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Content", header_style)));
            lines.push(Line::from(""));
            for line in content.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Gray),
                )));
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "(binary file — preview not available)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let width = inner.width.max(1) as usize;
        let mut wrapped_lines: u16 = 0;
        for line in &lines {
            let line_width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            if line_width == 0 {
                wrapped_lines += 1;
            } else {
                wrapped_lines += ((line_width.saturating_sub(1)) / width + 1) as u16;
            }
        }

        let max_scroll = wrapped_lines.saturating_sub(inner.height);
        self.detail_total_lines = max_scroll;

        if self.detail_scroll > max_scroll {
            self.detail_scroll = max_scroll;
        }

        let p = Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.detail_scroll, 0));
        frame.render_widget(p, inner);
    }

    fn render_filter(&self, frame: &mut Frame, area: Rect) {
        let filter_area = ratatui::layout::Rect {
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
}

fn detail_line(label: &str, value: &str, ls: Style, vs: Style) -> Line<'static> {
    let label_width = 20;
    Line::from(vec![
        Span::styled(format!("{:<width$}", label, width = label_width), ls),
        Span::styled(value.to_string(), vs),
    ])
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
