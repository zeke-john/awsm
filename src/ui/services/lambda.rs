use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::Action;
use crate::aws::lambda::{FunctionDetail, FunctionInfo, format_size};
use crate::ui::services::ServiceComponent;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LambdaScreen {
    Functions,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Runtime,
    Modified,
    Memory,
    Timeout,
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
pub struct LambdaView {
    screen: LambdaScreen,
    functions: Vec<FunctionInfo>,
    pub detail: Option<FunctionDetail>,
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
}

impl Default for LambdaView {
    fn default() -> Self {
        Self {
            screen: LambdaScreen::Functions,
            functions: Vec::new(),
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
        }
    }
}

impl LambdaView {
    pub fn set_functions(&mut self, functions: Vec<FunctionInfo>) {
        self.functions = functions;
        self.loading = false;
        self.error = None;
        self.selected = 0;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    pub fn enter_detail(&mut self) {
        self.screen = LambdaScreen::Detail;
        self.detail = None;
        self.detail_scroll = 0;
        self.loading = true;
        self.filter.clear();
        self.filtering = false;
    }

    pub fn set_detail(&mut self, detail: FunctionDetail) {
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
            LambdaScreen::Detail => {
                self.screen = LambdaScreen::Functions;
                self.detail = None;
                self.loading = false;
                true
            }
            LambdaScreen::Functions => false,
        }
    }

    pub fn selected_function(&self) -> Option<&FunctionInfo> {
        let filtered = self.filtered_functions();
        filtered.into_iter().nth(self.selected)
    }

    pub fn is_editing(&self) -> bool {
        self.filtering
    }

    pub fn needs_function_load(&self) -> bool {
        self.screen == LambdaScreen::Functions && self.loading
    }

    pub fn screen_type(&self) -> &str {
        match self.screen {
            LambdaScreen::Functions => "functions",
            LambdaScreen::Detail => "detail",
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.screen {
            LambdaScreen::Functions => "Lambda > Functions".to_string(),
            LambdaScreen::Detail => {
                let name = self
                    .detail
                    .as_ref()
                    .map(|d| d.name.as_str())
                    .unwrap_or("...");
                format!("Lambda > {}", name)
            }
        }
    }

    fn filtered_functions(&self) -> Vec<&FunctionInfo> {
        let mut result: Vec<&FunctionInfo> = if self.filter.is_empty() {
            self.functions.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.functions
                .iter()
                .filter(|func| {
                    func.name.to_lowercase().contains(&f)
                        || func.runtime.to_lowercase().contains(&f)
                        || func.description.to_lowercase().contains(&f)
                })
                .collect()
        };

        result.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Runtime => a.runtime.cmp(&b.runtime),
                SortColumn::Modified => a.last_modified.cmp(&b.last_modified),
                SortColumn::Memory => a.memory.cmp(&b.memory),
                SortColumn::Timeout => a.timeout.cmp(&b.timeout),
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
        self.filtered_functions().len()
    }
}

impl ServiceComponent for LambdaView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.screen == LambdaScreen::Detail {
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
                    SortColumn::Name => SortColumn::Runtime,
                    SortColumn::Runtime => SortColumn::Modified,
                    SortColumn::Modified => SortColumn::Memory,
                    SortColumn::Memory => SortColumn::Timeout,
                    SortColumn::Timeout => SortColumn::Name,
                };
                self.selected = 0;
            }
            KeyCode::Char('S') => {
                self.sort_column = match self.sort_column {
                    SortColumn::Name => SortColumn::Timeout,
                    SortColumn::Runtime => SortColumn::Name,
                    SortColumn::Modified => SortColumn::Runtime,
                    SortColumn::Memory => SortColumn::Modified,
                    SortColumn::Timeout => SortColumn::Memory,
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
            LambdaScreen::Functions => self.render_functions(frame, area),
            LambdaScreen::Detail => self.render_detail(frame, area),
        }
    }

    fn name(&self) -> &'static str {
        "Lambda"
    }
}

impl LambdaView {
    fn render_functions(&self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading functions...",
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

        let filtered = self.filtered_functions();

        let w = area.width as usize;
        let runtime_col = 14;
        let modified_col = 12;
        let memory_col = 8;
        let timeout_col = 8;
        let fixed = runtime_col + modified_col + memory_col + timeout_col + 8;
        let name_col = w.saturating_sub(fixed);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No functions found"
            } else {
                "  No matching functions"
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

        let col_header = |label: &str, col: SortColumn, width: usize| -> Span<'static> {
            let text = if self.sort_column == col {
                format!("{} {}", label, self.sort_dir.arrow())
            } else {
                label.to_string()
            };
            let style = if self.sort_column == col {
                active_hdr
            } else {
                header_style
            };
            Span::styled(format!("{:>width$}", text, width = width), style)
        };

        let name_hdr = if self.sort_column == SortColumn::Name {
            format!("Name {}", self.sort_dir.arrow())
        } else {
            "Name".to_string()
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
            col_header("Runtime", SortColumn::Runtime, runtime_col),
            col_header("Memory", SortColumn::Memory, memory_col),
            col_header("Timeout", SortColumn::Timeout, timeout_col),
            col_header("Modified", SortColumn::Modified, modified_col),
            Span::styled("  ", header_style),
        ]))];

        for (i, func) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let modified = if func.last_modified.len() >= 10 {
                &func.last_modified[..10]
            } else {
                &func.last_modified
            };

            let memory_str = format!("{}MB", func.memory);
            let timeout_str = format!("{}s", func.timeout);
            let name = truncate_str(&func.name, name_col);

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(
                    format!("{:>width$}", func.runtime, width = runtime_col),
                    style,
                ),
                Span::styled(
                    format!("{:>width$}", memory_str, width = memory_col),
                    style,
                ),
                Span::styled(
                    format!("{:>width$}", timeout_str, width = timeout_col),
                    style,
                ),
                Span::styled(
                    format!("{:>width$}", modified, width = modified_col),
                    style,
                ),
                Span::styled("  ", style),
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

        let code_size_str = format_size(detail.code_size);
        let memory_str = format!("{} MB", detail.memory);
        let timeout_str = format!("{} seconds", detail.timeout);
        let arch_str = if detail.architectures.is_empty() {
            "x86_64".to_string()
        } else {
            detail.architectures.join(", ")
        };
        let ephemeral_str = detail
            .ephemeral_storage
            .map(|s| format!("{} MB", s))
            .unwrap_or_else(|| "512 MB".to_string());

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled("Configuration", header_style)),
            Line::from(""),
            detail_line("Function Name", &detail.name, label_style, value_style),
            detail_line(
                "ARN",
                &detail.arn,
                label_style,
                Style::default().fg(Color::Blue),
            ),
            detail_line("Description", &detail.description, label_style, value_style),
            detail_line("Runtime", &detail.runtime, label_style, value_style),
            detail_line("Handler", &detail.handler, label_style, value_style),
            detail_line("Architecture", &arch_str, label_style, value_style),
            detail_line("Package Type", &detail.package_type, label_style, value_style),
            detail_line("Memory", &memory_str, label_style, value_style),
            detail_line("Timeout", &timeout_str, label_style, value_style),
            detail_line(
                "Ephemeral Storage",
                &ephemeral_str,
                label_style,
                value_style,
            ),
            detail_line("Code Size", &code_size_str, label_style, value_style),
            detail_line("Code SHA256", &detail.code_sha256, label_style, value_style),
            detail_line("Last Modified", &detail.last_modified, label_style, value_style),
            detail_line(
                "Role",
                &detail.role,
                label_style,
                Style::default().fg(Color::Blue),
            ),
        ];

        if let Some(ref state) = detail.state {
            let state_style = match state.as_str() {
                "Active" => Style::default().fg(Color::Green),
                "Failed" => Style::default().fg(Color::Red),
                _ => value_style,
            };
            lines.push(detail_line("State", state, label_style, state_style));
        }
        if let Some(ref reason) = detail.state_reason {
            if !reason.is_empty() {
                lines.push(detail_line("State Reason", reason, label_style, value_style));
            }
        }
        if let Some(ref tracing) = detail.tracing_mode {
            lines.push(detail_line("Tracing", tracing, label_style, value_style));
        }
        if let Some(ref dlq) = detail.dead_letter_arn {
            lines.push(detail_line("Dead Letter ARN", dlq, label_style, value_style));
        }

        let has_vpc = detail.vpc_id.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
            || !detail.subnet_ids.is_empty()
            || !detail.security_group_ids.is_empty();

        if has_vpc {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("VPC Configuration", header_style)));
            lines.push(Line::from(""));
            if let Some(ref vpc) = detail.vpc_id {
                if !vpc.is_empty() {
                    lines.push(detail_line("VPC ID", vpc, label_style, value_style));
                }
            }
            if !detail.subnet_ids.is_empty() {
                lines.push(detail_line(
                    "Subnets",
                    &detail.subnet_ids.join(", "),
                    label_style,
                    value_style,
                ));
            }
            if !detail.security_group_ids.is_empty() {
                lines.push(detail_line(
                    "Security Groups",
                    &detail.security_group_ids.join(", "),
                    label_style,
                    value_style,
                ));
            }
        }

        if !detail.env_vars.is_empty() {
            let max_key = detail.env_vars.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            let env_label_w = (max_key + 2).max(20);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Environment Variables ({})", detail.env_vars.len()),
                header_style,
            )));
            lines.push(Line::from(""));
            for (k, v) in &detail.env_vars {
                lines.push(detail_line_w(k, v, label_style, value_style, env_label_w));
            }
        }

        if !detail.layers.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Layers ({})", detail.layers.len()),
                header_style,
            )));
            lines.push(Line::from(""));
            for (i, layer) in detail.layers.iter().enumerate() {
                let label = format!("Layer {}", i + 1);
                lines.push(detail_line(&label, layer, label_style, value_style));
            }
        }

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

fn detail_line_w(label: &str, value: &str, ls: Style, vs: Style, label_width: usize) -> Line<'static> {
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
