use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::Action;
use crate::aws::secrets::{SecretDetail, SecretInfo};
use crate::ui::services::ServiceComponent;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SecretsScreen {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    LastAccessed,
    LastChanged,
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
pub struct SecretsManagerView {
    screen: SecretsScreen,
    secrets: Vec<SecretInfo>,
    pub detail: Option<SecretDetail>,
    pub detail_scroll: u16,
    selected: usize,
    loading: bool,
    error: Option<String>,
    filter: String,
    filtering: bool,
    pending_g: bool,
    detail_total_lines: u16,
    sort_column: SortColumn,
    sort_dir: SortDir,
    show_secret: bool,
}

impl Default for SecretsManagerView {
    fn default() -> Self {
        Self {
            screen: SecretsScreen::List,
            secrets: Vec::new(),
            detail: None,
            detail_scroll: 0,
            selected: 0,
            loading: true,
            error: None,
            filter: String::new(),
            filtering: false,
            pending_g: false,
            detail_total_lines: 0,
            sort_column: SortColumn::Name,
            sort_dir: SortDir::Asc,
            show_secret: false,
        }
    }
}

impl SecretsManagerView {
    pub fn set_secrets(&mut self, secrets: Vec<SecretInfo>) {
        self.secrets = secrets;
        self.loading = false;
        self.error = None;
        self.selected = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    pub fn enter_detail(&mut self) {
        self.screen = SecretsScreen::Detail;
        self.detail = None;
        self.detail_scroll = 0;
        self.loading = true;
        self.show_secret = false;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn set_detail(&mut self, detail: SecretDetail) {
        self.detail = Some(detail);
        self.loading = false;
        self.error = None;
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
            SecretsScreen::Detail => {
                self.screen = SecretsScreen::List;
                self.detail = None;
                self.show_secret = false;
                self.loading = false;
                true
            }
            SecretsScreen::List => false,
        }
    }

    pub fn selected_secret(&self) -> Option<&SecretInfo> {
        let filtered = self.filtered_secrets();
        filtered.into_iter().nth(self.selected)
    }

    pub fn is_editing(&self) -> bool {
        self.filtering
    }

    pub fn needs_secret_load(&self) -> bool {
        self.screen == SecretsScreen::List && self.loading
    }

    pub fn screen_type(&self) -> &str {
        match self.screen {
            SecretsScreen::List => "list",
            SecretsScreen::Detail => "detail",
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.screen {
            SecretsScreen::List => "Secrets Manager > Secrets".to_string(),
            SecretsScreen::Detail => {
                let name = self
                    .detail
                    .as_ref()
                    .map(|d| d.name.as_str())
                    .unwrap_or("...");
                format!("Secrets Manager > {}", name)
            }
        }
    }

    fn filtered_secrets(&self) -> Vec<&SecretInfo> {
        let mut result: Vec<&SecretInfo> = if self.filter.is_empty() {
            self.secrets.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.secrets
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&f)
                        || s.description.to_lowercase().contains(&f)
                })
                .collect()
        };

        result.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::LastAccessed => a.last_accessed.cmp(&b.last_accessed),
                SortColumn::LastChanged => a.last_changed.cmp(&b.last_changed),
            };
            if self.sort_dir == SortDir::Desc {
                ord.reverse()
            } else {
                ord
            }
        });
        result
    }

    fn item_count(&self) -> usize {
        self.filtered_secrets().len()
    }
}

impl ServiceComponent for SecretsManagerView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.screen == SecretsScreen::Detail {
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
                KeyCode::Char('d')
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    self.pending_g = false;
                    self.detail_scroll = self.detail_scroll.saturating_add(20);
                }
                KeyCode::Char('u')
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
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
                KeyCode::Char('s') => {
                    self.pending_g = false;
                    self.show_secret = !self.show_secret;
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
                self.pending_g = false;
                let count = self.item_count();
                if count > 0 && self.selected + 1 < count {
                    self.selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pending_g = false;
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
            KeyCode::Char('d')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.pending_g = false;
                let count = self.item_count();
                if count > 0 {
                    self.selected = (self.selected + 20).min(count - 1);
                }
            }
            KeyCode::Char('u')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.pending_g = false;
                self.selected = self.selected.saturating_sub(20);
            }
            KeyCode::Char('s') => {
                self.sort_column = match self.sort_column {
                    SortColumn::Name => SortColumn::LastChanged,
                    SortColumn::LastChanged => SortColumn::LastAccessed,
                    SortColumn::LastAccessed => SortColumn::Name,
                };
                self.selected = 0;
            }
            KeyCode::Char('S') => {
                self.sort_column = match self.sort_column {
                    SortColumn::Name => SortColumn::LastAccessed,
                    SortColumn::LastChanged => SortColumn::Name,
                    SortColumn::LastAccessed => SortColumn::LastChanged,
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
            SecretsScreen::List => self.render_list(frame, area),
            SecretsScreen::Detail => self.render_detail(frame, area),
        }
    }

    fn name(&self) -> &'static str {
        "Secrets Manager"
    }
}

impl SecretsManagerView {
    fn render_list(&self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading secrets...",
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

        let filtered = self.filtered_secrets();

        let w = area.width as usize;
        let changed_col = 12;
        let accessed_col = 12;
        let fixed = changed_col + accessed_col + 8;
        let name_col = w.saturating_sub(fixed);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No secrets found"
            } else {
                "  No matching secrets"
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

        let name_hdr = if self.sort_column == SortColumn::Name {
            format!("Name {}", self.sort_dir.arrow())
        } else {
            "Name".to_string()
        };
        let changed_hdr = if self.sort_column == SortColumn::LastChanged {
            format!("Changed {}", self.sort_dir.arrow())
        } else {
            "Changed".to_string()
        };
        let accessed_hdr = if self.sort_column == SortColumn::LastAccessed {
            format!("Accessed {}", self.sort_dir.arrow())
        } else {
            "Accessed".to_string()
        };

        let mut items: Vec<ListItem> = vec![ListItem::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", name_hdr, width = name_col),
                if self.sort_column == SortColumn::Name {
                    active_hdr
                } else {
                    header_style
                },
            ),
            Span::styled(
                format!("{:>width$}", changed_hdr, width = changed_col),
                if self.sort_column == SortColumn::LastChanged {
                    active_hdr
                } else {
                    header_style
                },
            ),
            Span::styled(
                format!("{:>width$}  ", accessed_hdr, width = accessed_col),
                if self.sort_column == SortColumn::LastAccessed {
                    active_hdr
                } else {
                    header_style
                },
            ),
        ]))];

        for (i, secret) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let changed = if secret.last_changed.len() >= 10 {
                &secret.last_changed[..10]
            } else if secret.last_changed.is_empty() {
                "-"
            } else {
                &secret.last_changed
            };
            let accessed = if secret.last_accessed.len() >= 10 {
                &secret.last_accessed[..10]
            } else if secret.last_accessed.is_empty() {
                "-"
            } else {
                &secret.last_accessed
            };

            let name = truncate_str(&secret.name, name_col);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(
                    format!("{:>width$}", changed, width = changed_col),
                    style,
                ),
                Span::styled(
                    format!("{:>width$}  ", accessed, width = accessed_col),
                    style,
                ),
            ])));
        }

        let list = List::new(items);
        let mut state = ListState::default().with_selected(Some(self.selected + 1));
        frame.render_stateful_widget(list, area, &mut state);

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

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled("Secret Details", header_style)),
            Line::from(""),
            detail_line("Name", &detail.name, label_style, value_style),
            detail_line(
                "ARN",
                &detail.arn,
                label_style,
                Style::default().fg(Color::Blue),
            ),
        ];

        if !detail.description.is_empty() {
            lines.push(detail_line(
                "Description",
                &detail.description,
                label_style,
                value_style,
            ));
        }

        if let Some(ref kms) = detail.kms_key_id {
            lines.push(detail_line("KMS Key ID", kms, label_style, value_style));
        }
        if let Some(ref created) = detail.created {
            lines.push(detail_line("Created", created, label_style, value_style));
        }
        if let Some(ref changed) = detail.last_changed {
            lines.push(detail_line("Last Changed", changed, label_style, value_style));
        }
        if let Some(ref accessed) = detail.last_accessed {
            lines.push(detail_line(
                "Last Accessed",
                accessed,
                label_style,
                value_style,
            ));
        }
        if let Some(ref deleted) = detail.deleted_date {
            lines.push(detail_line(
                "Deletion Date",
                deleted,
                label_style,
                Style::default().fg(Color::Red),
            ));
        }

        // Rotation
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Rotation", header_style)));
        lines.push(Line::from(""));
        let rotation_str = if detail.rotation_enabled {
            "Enabled"
        } else {
            "Disabled"
        };
        let rot_style = if detail.rotation_enabled {
            Style::default().fg(Color::Green)
        } else {
            value_style
        };
        lines.push(detail_line("Rotation", rotation_str, label_style, rot_style));
        if let Some(days) = detail.rotation_days {
            lines.push(detail_line(
                "Rotation Interval",
                &format!("{} days", days),
                label_style,
                value_style,
            ));
        }
        if let Some(ref arn) = detail.rotation_lambda_arn {
            lines.push(detail_line(
                "Rotation Lambda",
                arn,
                label_style,
                value_style,
            ));
        }
        if let Some(ref last) = detail.last_rotated {
            lines.push(detail_line("Last Rotated", last, label_style, value_style));
        }

        // Secret Value
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Secret Value", header_style)));
        lines.push(Line::from(""));

        let toggle_key = Style::default()
            .fg(Color::Rgb(255, 165, 0))
            .add_modifier(Modifier::BOLD);
        let toggle_text = Style::default().fg(Color::Rgb(255, 165, 0));

        if detail.secret_binary {
            lines.push(Line::from(Span::styled(
                "(binary secret — preview not available)",
                Style::default().fg(Color::DarkGray),
            )));
        } else if let Some(ref val) = detail.secret_value {
            if self.show_secret {
                // Try to pretty-print JSON
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(val) {
                    if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
                        for line in pretty.lines() {
                            lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::White),
                            )));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            val.clone(),
                            Style::default().fg(Color::White),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        val.clone(),
                        Style::default().fg(Color::White),
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("[s]", toggle_key),
                    Span::styled(" Hide secret value", toggle_text),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    "••••••••••••••••••••••••••••••••",
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("[s]", toggle_key),
                    Span::styled(" Reveal secret value", toggle_text),
                ]));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "(no value available)",
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Versions
        if !detail.version_ids.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Versions ({})", detail.version_ids.len()),
                header_style,
            )));
            lines.push(Line::from(""));
            for (vid, stages) in &detail.version_ids {
                let stage_str = if stages.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", stages.join(", "))
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        truncate_str(vid, 40),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(stage_str, Style::default().fg(Color::Cyan)),
                ]));
            }
        }

        // Tags
        if !detail.tags.is_empty() {
            let max_key = detail.tags.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            let tag_label_w = (max_key + 2).max(20);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Tags ({})", detail.tags.len()),
                header_style,
            )));
            lines.push(Line::from(""));
            for (k, v) in &detail.tags {
                lines.push(detail_line_w(k, v, label_style, value_style, tag_label_w));
            }
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
}

fn detail_line(label: &str, value: &str, ls: Style, vs: Style) -> Line<'static> {
    detail_line_w(label, value, ls, vs, 20)
}

fn detail_line_w(
    label: &str,
    value: &str,
    ls: Style,
    vs: Style,
    label_width: usize,
) -> Line<'static> {
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
