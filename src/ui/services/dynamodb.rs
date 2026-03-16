use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::app::Action;
use crate::aws::dynamodb::{
    DynamoItem, IndexInfo, ScanResult, TableDetail, TableInfo, attribute_value_to_json, format_size,
};
use crate::ui::services::ServiceComponent;

#[derive(Debug, Clone)]
struct DdbFilter {
    attribute: String,
    condition: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DdbScreen {
    Tables,
    Items,
    Detail,
}

#[derive(Debug)]
pub struct DynamoDbView {
    screen: DdbScreen,
    tables: Vec<TableInfo>,
    table_detail: Option<TableDetail>,
    items_result: Option<ScanResult>,
    visible_columns: Vec<String>,
    col_offset: usize,
    selected: usize,
    tables_list_state: ListState,
    loading: bool,
    error: Option<String>,
    filter: String,
    filtering: bool,
    pending_g: bool,
    detail_scroll: u16,
    detail_total_lines: u16,
    detail_json: Option<String>,
    // query state
    pub active_table: String,
    pub active_index: Option<String>,
    pub query_mode: bool, // false = scan, true = query
    pub query_pk_value: String,
    pub query_sk_condition: String,
    pub query_sk_value: String,
    pub query_descending: bool,
    pub query_summary: Option<String>,
    sort_col_idx: Option<usize>,
    sort_ascending: bool,
    col_width_override: usize,
    last_visible_scroll_cols: usize,
    items_list_state: ListState,
    // index picker
    index_picker_open: bool,
    index_picker_selected: usize,
    // query builder
    query_builder_open: bool,
    query_builder_field: usize,
    query_builder_editing: bool,
    query_filters: Vec<DdbFilter>,
    // pagination
    all_pages: Vec<ScanResult>,
    current_page: usize,
}

impl Default for DynamoDbView {
    fn default() -> Self {
        Self {
            screen: DdbScreen::Tables,
            tables: Vec::new(),
            table_detail: None,
            items_result: None,
            visible_columns: Vec::new(),
            col_offset: 0,
            selected: 0,
            tables_list_state: ListState::default(),
            loading: true,
            error: None,
            filter: String::new(),
            filtering: false,
            pending_g: false,
            detail_scroll: 0,
            detail_total_lines: 0,
            detail_json: None,
            active_table: String::new(),
            active_index: None,
            query_mode: true,
            query_pk_value: String::new(),
            query_sk_condition: "=".to_string(),
            query_sk_value: String::new(),
            query_descending: false,
            query_summary: None,
            sort_col_idx: None,
            sort_ascending: true,
            col_width_override: 0,
            last_visible_scroll_cols: 4,
            items_list_state: ListState::default(),
            index_picker_open: false,
            index_picker_selected: 0,
            query_builder_open: false,
            query_builder_field: 0,
            query_builder_editing: false,
            query_filters: Vec::new(),
            all_pages: Vec::new(),
            current_page: 0,
        }
    }
}

impl DynamoDbView {
    pub fn set_tables(&mut self, tables: Vec<TableInfo>) {
        self.tables = tables;
        self.loading = false;
        self.error = None;
        self.selected = 0;
        self.tables_list_state = ListState::default();
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    pub fn set_loading(&mut self) {
        self.loading = true;
        self.error = None;
    }

    pub fn enter_table(&mut self, name: String) {
        self.active_table = name;
        self.active_index = None;
        self.screen = DdbScreen::Items;
        self.selected = 0;
        self.loading = true;
        self.filter.clear();
        self.filtering = false;
        self.col_offset = 0;
        self.items_result = None;
        self.all_pages.clear();
        self.current_page = 0;
        *self.items_list_state.offset_mut() = 0;
        self.query_mode = true;
        self.query_pk_value.clear();
        self.query_sk_value.clear();
        self.query_summary = None;
        self.sort_col_idx = None;
        self.sort_ascending = true;
        self.col_width_override = 0;
    }

    pub fn set_table_detail(&mut self, detail: TableDetail) {
        self.table_detail = Some(detail);
    }

    pub fn set_items(&mut self, result: ScanResult) {
        self.visible_columns = self.order_columns(&result.columns);
        self.all_pages = vec![result.clone()];
        self.current_page = 0;
        self.items_result = Some(result);
        self.loading = false;
        self.error = None;
        self.selected = 0;
        self.col_offset = 0;
        *self.items_list_state.offset_mut() = 0;
    }

    pub fn add_page(&mut self, result: ScanResult) {
        for col in &result.columns {
            if !self.visible_columns.contains(col) {
                self.visible_columns.push(col.clone());
            }
        }
        self.all_pages.push(result.clone());
        self.current_page = self.all_pages.len() - 1;
        self.items_result = Some(result);
        self.loading = false;
        self.error = None;
        self.selected = 0;
        self.col_offset = 0;
        *self.items_list_state.offset_mut() = 0;
    }

    pub fn next_page(&mut self) {
        if self.current_page + 1 < self.all_pages.len() {
            self.current_page += 1;
            self.items_result = Some(self.all_pages[self.current_page].clone());
            self.selected = 0;
            self.col_offset = 0;
            *self.items_list_state.offset_mut() = 0;
        }
    }

    pub fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.items_result = Some(self.all_pages[self.current_page].clone());
            self.selected = 0;
            self.col_offset = 0;
            *self.items_list_state.offset_mut() = 0;
        }
    }

    pub fn has_next_page(&self) -> bool {
        self.current_page + 1 < self.all_pages.len()
            || self
                .all_pages
                .last()
                .and_then(|p| p.last_key.as_ref())
                .is_some()
    }

    pub fn needs_fetch_next_page(&self) -> bool {
        self.current_page + 1 >= self.all_pages.len()
            && self
                .all_pages
                .last()
                .and_then(|p| p.last_key.as_ref())
                .is_some()
    }

    pub fn last_key(&self) -> Option<&BTreeMap<String, aws_sdk_dynamodb::types::AttributeValue>> {
        self.all_pages.last().and_then(|r| r.last_key.as_ref())
    }

    pub fn page_info(&self) -> (usize, usize) {
        let current = self.current_page + 1;
        let page_size = self
            .items_result
            .as_ref()
            .map(|r| r.items.len())
            .unwrap_or(300)
            .max(1);
        let total = self
            .table_detail
            .as_ref()
            .map(|d| ((d.item_count as usize) + page_size - 1) / page_size)
            .unwrap_or(self.all_pages.len());
        (current, total.max(1))
    }

    fn order_columns(&self, columns: &[String]) -> Vec<String> {
        let mut ordered = Vec::new();

        if let Some(ref detail) = self.table_detail {
            // matching the AWS console column order ->
            // 1. Table PK (always first / sticky column)
            // 2. Index PK (if on a GSI and different from table PK)
            // 3. Index SK (if on a GSI)
            // 4. Table SK (if different from above)
            // 5. all the other columns yuh

            // 1. Table PK always first (sticky)
            if columns.contains(&detail.partition_key) {
                ordered.push(detail.partition_key.clone());
            }

            if let Some(ref idx_name) = self.active_index {
                if let Some(idx) = detail.indexes.iter().find(|i| &i.name == idx_name) {
                    // 2. Index PK
                    if columns.contains(&idx.partition_key) && !ordered.contains(&idx.partition_key)
                    {
                        ordered.push(idx.partition_key.clone());
                    }
                    // 3. Index SK
                    if let Some(ref sk) = idx.sort_key {
                        if columns.contains(sk) && !ordered.contains(sk) {
                            ordered.push(sk.clone());
                        }
                    }
                }
            }

            // 4. Table SK
            if let Some(ref sk_name) = detail.sort_key {
                if columns.contains(sk_name) && !ordered.contains(sk_name) {
                    ordered.push(sk_name.clone());
                }
            }

            // 5. Remaining columns in original order
            for col in columns {
                if !ordered.contains(col) {
                    ordered.push(col.clone());
                }
            }
        } else {
            ordered.extend(columns.iter().cloned());
        }

        ordered
    }

    pub fn needs_table_load(&self) -> bool {
        self.screen == DdbScreen::Tables && self.loading
    }

    pub fn needs_items_load(&self) -> bool {
        self.screen == DdbScreen::Items && self.loading
    }

    pub fn selected_table(&self) -> Option<&TableInfo> {
        let filtered = self.filtered_tables();
        filtered.into_iter().nth(self.selected)
    }

    pub fn selected_item(&self) -> Option<&DynamoItem> {
        if let Some(ref result) = self.items_result {
            let filtered = self.filtered_items(&result.items);
            filtered.into_iter().nth(self.selected)
        } else {
            None
        }
    }

    pub fn is_editing(&self) -> bool {
        self.query_builder_editing || self.filtering
    }

    pub fn has_overlay(&self) -> bool {
        self.query_builder_open || self.index_picker_open
    }

    pub fn filter_tuples(&self) -> Vec<(String, String, String)> {
        self.query_filters
            .iter()
            .filter(|f| !f.attribute.is_empty() && !f.value.is_empty())
            .map(|f| (f.attribute.clone(), f.condition.clone(), f.value.clone()))
            .collect()
    }

    pub fn table_detail(&self) -> Option<&TableDetail> {
        self.table_detail.as_ref()
    }

    pub fn screen_type(&self) -> &str {
        match self.screen {
            DdbScreen::Tables => "tables",
            DdbScreen::Items => "items",
            DdbScreen::Detail => "detail",
        }
    }

    pub fn breadcrumb(&self) -> String {
        match self.screen {
            DdbScreen::Tables => {
                let count = self.tables.len();
                format!("DynamoDB > Tables ({})", count)
            }
            DdbScreen::Items => {
                let idx = self.active_index.as_deref().unwrap_or("Primary");
                format!("DynamoDB > {} ({})", self.active_table, idx)
            }
            DdbScreen::Detail => format!("DynamoDB > {} > Item", self.active_table),
        }
    }

    fn filtered_tables(&self) -> Vec<&TableInfo> {
        let mut result: Vec<&TableInfo> = if self.filter.is_empty() {
            self.tables.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            self.tables
                .iter()
                .filter(|t| t.name.to_lowercase().contains(&f))
                .collect()
        };

        if let Some(col) = self.sort_col_idx {
            result.sort_by(|a, b| {
                let ord = match col {
                    0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    1 => a.status.cmp(&b.status),
                    2 => a.item_count.cmp(&b.item_count),
                    3 => a.size_bytes.cmp(&b.size_bytes),
                    _ => std::cmp::Ordering::Equal,
                };
                if self.sort_ascending {
                    ord
                } else {
                    ord.reverse()
                }
            });
        }

        result
    }

    fn filtered_items<'a>(&self, items: &'a [DynamoItem]) -> Vec<&'a DynamoItem> {
        let mut result: Vec<&DynamoItem> = if self.filter.is_empty() {
            items.iter().collect()
        } else {
            let f = self.filter.to_lowercase();
            items
                .iter()
                .filter(|item| {
                    item.attributes
                        .values()
                        .any(|v| v.to_lowercase().contains(&f))
                })
                .collect()
        };

        if let Some(col_idx) = self.sort_col_idx {
            if let Some(col_name) = self.visible_columns.get(col_idx) {
                let col = col_name.clone();
                result.sort_by(|a, b| {
                    let va = a.attributes.get(&col).map(|s| s.as_str()).unwrap_or("");
                    let vb = b.attributes.get(&col).map(|s| s.as_str()).unwrap_or("");
                    let ord = smart_cmp(va, vb);
                    if self.sort_ascending {
                        ord
                    } else {
                        ord.reverse()
                    }
                });
            }
        }

        result
    }

    fn item_count(&self) -> usize {
        match self.screen {
            DdbScreen::Tables => self.filtered_tables().len(),
            DdbScreen::Items => {
                if let Some(ref result) = self.items_result {
                    self.filtered_items(&result.items).len()
                } else {
                    0
                }
            }
            DdbScreen::Detail => 0,
        }
    }

    pub fn enter_detail(&mut self) {
        if let Some(item) = self.selected_item().cloned() {
            let json_map: serde_json::Map<String, serde_json::Value> = item
                .raw
                .iter()
                .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
                .collect();
            let json = serde_json::to_string_pretty(&serde_json::Value::Object(json_map))
                .unwrap_or_else(|_| "{}".to_string());
            self.detail_json = Some(json);
            self.screen = DdbScreen::Detail;
            self.detail_scroll = 0;
            self.filter.clear();
            self.filtering = false;
        }
    }

    fn scroll_to_sort_col(&mut self) {
        if self.screen != DdbScreen::Items {
            return;
        }
        let idx = match self.sort_col_idx {
            Some(i) => i,
            None => return,
        };
        if idx == 0 {
            return;
        }
        let scroll_idx = idx - 1;
        if scroll_idx < self.col_offset {
            self.col_offset = scroll_idx;
        } else {
            let visible = self.last_visible_scroll_cols.max(1);
            if scroll_idx >= self.col_offset + visible {
                self.col_offset = scroll_idx - visible + 1;
            }
        }
    }

    fn go_back(&mut self) -> bool {
        if self.filtering {
            self.filtering = false;
            self.filter.clear();
            return true;
        }
        self.filter.clear();
        self.filtering = false;
        match self.screen {
            DdbScreen::Detail => {
                self.screen = DdbScreen::Items;
                self.detail_json = None;
                true
            }
            DdbScreen::Items => {
                self.screen = DdbScreen::Tables;
                self.selected = 0;
                self.loading = false;
                self.query_summary = None;
                true
            }
            DdbScreen::Tables => false,
        }
    }
}

impl ServiceComponent for DynamoDbView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.screen == DdbScreen::Detail {
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

        if self.index_picker_open {
            let index_count = 1 + self
                .table_detail
                .as_ref()
                .map(|d| d.indexes.len())
                .unwrap_or(0);
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.index_picker_selected + 1 < index_count {
                        self.index_picker_selected += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.index_picker_selected > 0 {
                        self.index_picker_selected -= 1;
                    }
                }
                KeyCode::Enter => {
                    if self.index_picker_selected == 0 {
                        self.active_index = None;
                    } else if let Some(ref detail) = self.table_detail {
                        if let Some(idx) = detail.indexes.get(self.index_picker_selected - 1) {
                            self.active_index = Some(idx.name.clone());
                        }
                    }
                    self.index_picker_open = false;
                    self.loading = true;
                    self.all_pages.clear();
                    self.current_page = 0;
                    *self.items_list_state.offset_mut() = 0;
                    self.items_result = None;
                    self.selected = 0;
                    self.sort_col_idx = None;
                    self.filter.clear();
                    return Some(Action::DdbSwitchIndex);
                }
                KeyCode::Esc | KeyCode::Char('i') => {
                    self.index_picker_open = false;
                }
                _ => {}
            }
            return Some(Action::None);
        }

        if self.query_builder_open {
            return self.handle_query_builder_key(key);
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
            KeyCode::Char('l') | KeyCode::Right => {
                if self.screen == DdbScreen::Items && self.visible_columns.len() > 1 {
                    let max = self.visible_columns.len().saturating_sub(2);
                    self.col_offset = (self.col_offset + 1).min(max);
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if self.screen == DdbScreen::Items && self.col_offset > 0 {
                    self.col_offset = self.col_offset.saturating_sub(1);
                } else if self.screen != DdbScreen::Items {
                    if self.go_back() {
                        return Some(Action::ServiceBack);
                    } else {
                        return None;
                    }
                }
            }
            KeyCode::Char('s') => {
                self.pending_g = false;
                let max_col = if self.screen == DdbScreen::Items {
                    self.visible_columns.len()
                } else if self.screen == DdbScreen::Tables {
                    4
                } else {
                    0
                };
                if max_col > 0 {
                    match self.sort_col_idx {
                        None => {
                            self.sort_col_idx = Some(0);
                            self.sort_ascending = true;
                        }
                        Some(idx) if idx + 1 < max_col => {
                            self.sort_col_idx = Some(idx + 1);
                        }
                        Some(_) => {
                            self.sort_col_idx = None;
                        }
                    }
                    self.selected = 0;
                    self.scroll_to_sort_col();
                }
            }
            KeyCode::Char('S') => {
                self.pending_g = false;
                let max_col = if self.screen == DdbScreen::Items {
                    self.visible_columns.len()
                } else if self.screen == DdbScreen::Tables {
                    4
                } else {
                    0
                };
                if max_col > 0 {
                    match self.sort_col_idx {
                        None => {
                            self.sort_col_idx = Some(max_col - 1);
                            self.sort_ascending = true;
                        }
                        Some(0) => {
                            self.sort_col_idx = None;
                        }
                        Some(idx) => {
                            self.sort_col_idx = Some(idx - 1);
                        }
                    }
                    self.selected = 0;
                    self.scroll_to_sort_col();
                }
            }
            KeyCode::Char('x') => {
                self.pending_g = false;
                if self.sort_col_idx.is_some() {
                    self.sort_ascending = !self.sort_ascending;
                    self.selected = 0;
                }
            }
            KeyCode::Char('X') => {
                self.pending_g = false;
                if self.screen == DdbScreen::Items && self.query_summary.is_some() {
                    self.query_mode = true;
                    self.query_pk_value.clear();
                    self.query_sk_value.clear();
                    self.query_summary = None;
                    self.loading = true;
                    self.all_pages.clear();
                    self.current_page = 0;
                    *self.items_list_state.offset_mut() = 0;
                    self.items_result = None;
                    self.selected = 0;
                    return Some(Action::DdbSwitchIndex);
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.screen == DdbScreen::Items {
                    if self.col_width_override == 0 {
                        self.col_width_override = 16;
                    }
                    self.col_width_override = (self.col_width_override + 4).min(60);
                }
            }
            KeyCode::Char('-') => {
                if self.screen == DdbScreen::Items {
                    if self.col_width_override == 0 {
                        self.col_width_override = 16;
                    }
                    self.col_width_override = self.col_width_override.saturating_sub(4).max(8);
                }
            }
            KeyCode::Char('0') => {
                if self.screen == DdbScreen::Items {
                    self.col_width_override = 0;
                }
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
            KeyCode::Char('i') => {
                if self.screen == DdbScreen::Items && self.table_detail.is_some() {
                    self.index_picker_open = true;
                    let current_idx = self.active_index.as_deref();
                    self.index_picker_selected = match current_idx {
                        None => 0,
                        Some(name) => self
                            .table_detail
                            .as_ref()
                            .and_then(|d| d.indexes.iter().position(|i| i.name == name))
                            .map(|pos| pos + 1)
                            .unwrap_or(0),
                    };
                }
            }
            KeyCode::Char('n') => {
                if self.screen == DdbScreen::Items && self.has_next_page() {
                    if self.needs_fetch_next_page() {
                        self.loading = true;
                        return Some(Action::DdbNextPage);
                    } else {
                        self.next_page();
                    }
                }
            }
            KeyCode::Char('N') => {
                if self.screen == DdbScreen::Items && self.current_page > 0 {
                    self.prev_page();
                }
            }
            KeyCode::Char('q') => {
                if self.screen == DdbScreen::Items && self.table_detail.is_some() {
                    self.query_builder_open = true;
                    self.query_builder_field = 0;
                    self.query_builder_editing = false;
                }
            }
            KeyCode::Enter => {
                return Some(Action::ServiceEnter);
            }
            KeyCode::Esc => {
                if self.screen == DdbScreen::Items && self.query_summary.is_some() {
                    // don't go back, just clear? no, go back
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
        match self.screen {
            DdbScreen::Tables => self.render_tables(frame, area),
            DdbScreen::Items => self.render_items(frame, area),
            DdbScreen::Detail => self.render_detail(frame, area),
        }
    }

    fn name(&self) -> &'static str {
        "DynamoDB"
    }
}

impl DynamoDbView {
    fn render_tables(&mut self, frame: &mut Frame, area: Rect) {
        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading tables...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, area);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            ))
            .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(p, area);
            return;
        }

        let filtered = self.filtered_tables();

        let w = area.width as usize;
        let status_col = 10;
        let items_col = 12;
        let size_col = 10;
        let name_col = w.saturating_sub(status_col + items_col + size_col + 6);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No tables found"
            } else {
                "  No matching tables"
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
        let sort_hdr = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let arrow = |idx: usize| -> &str {
            if self.sort_col_idx == Some(idx) {
                if self.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };
        let hdr_style = |idx: usize| -> Style {
            if self.sort_col_idx == Some(idx) {
                sort_hdr
            } else {
                header_style
            }
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(
                    "  {:<width$}",
                    format!("Name{}", arrow(0)),
                    width = name_col
                ),
                hdr_style(0),
            ),
            Span::styled(
                format!(
                    "{:<width$}",
                    format!("Status{}", arrow(1)),
                    width = status_col
                ),
                hdr_style(1),
            ),
            Span::styled(
                format!(
                    "{:>width$}",
                    format!("Items{}", arrow(2)),
                    width = items_col
                ),
                hdr_style(2),
            ),
            Span::styled(
                format!(
                    "{:>width$}  ",
                    format!("Size{}", arrow(3)),
                    width = size_col
                ),
                hdr_style(3),
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

        for (i, table) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let name = truncate_str(&table.name, name_col);
            let item_count = if table.item_count > 0 {
                format_number(table.item_count)
            } else {
                "-".to_string()
            };
            let size = if table.size_bytes > 0 {
                format_size(table.size_bytes)
            } else {
                "-".to_string()
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {:<width$}", name, width = name_col), style),
                Span::styled(
                    format!("{:<width$}", table.status, width = status_col),
                    if table.status == "ACTIVE" {
                        style
                    } else {
                        style.fg(Color::Yellow)
                    },
                ),
                Span::styled(format!("{:>width$}", item_count, width = items_col), style),
                Span::styled(format!("{:>width$}  ", size, width = size_col), style),
            ])));
        }

        let list = List::new(items);
        self.tables_list_state.select(Some(self.selected));
        let render_area = if self.filtering {
            Rect {
                height: data_area.height.saturating_sub(1),
                ..data_area
            }
        } else {
            data_area
        };
        frame.render_stateful_widget(list, render_area, &mut self.tables_list_state);

        if self.filtering {
            self.render_filter(frame, area);
        }
    }

    fn render_items(&mut self, frame: &mut Frame, area: Rect) {
        // Reserve last row for info bar
        let list_area = Rect {
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

        if self.loading {
            let p = Paragraph::new(Span::styled(
                "  Loading items...",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(p, list_area);
            return;
        }

        if let Some(ref err) = self.error {
            let p = Paragraph::new(Span::styled(
                format!("  Error: {}", err),
                Style::default().fg(Color::Red),
            ))
            .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(p, list_area);
            return;
        }

        let result = match &self.items_result {
            Some(r) => r,
            None => return,
        };

        let filtered = self.filtered_items(&result.items);

        if filtered.is_empty() {
            let msg = if self.filter.is_empty() {
                "  No items"
            } else {
                "  No matching items"
            };
            let p = Paragraph::new(Span::styled(msg, Style::default().fg(Color::DarkGray)));
            frame.render_widget(p, list_area);
            if self.filtering {
                self.render_filter(frame, list_area);
            }
            if self.index_picker_open {
                self.render_index_picker(frame, list_area);
            }
            if self.query_builder_open {
                self.render_query_builder(frame, list_area);
            }
            return;
        }

        let w = list_area.width as usize;
        let cols = &self.visible_columns;
        if cols.is_empty() {
            return;
        }

        let sep = "│";
        let dim = Style::default().fg(Color::Rgb(60, 60, 60));
        let row_line_style = Style::default().fg(Color::Rgb(40, 40, 40));

        let sticky_col = &cols[0];
        let scroll_start = (1 + self.col_offset).min(cols.len());

        let sticky_width = 14;
        // +1 for leading space, +1 for trailing space around sep
        let sticky_total = sticky_width + 3; // " val │"
        let remaining_w = w.saturating_sub(sticky_total);

        // Determine how many scrollable cols and their width
        let num_scroll_cols = cols.len().saturating_sub(1); // total scrollable
        let visible_scroll = if num_scroll_cols == 0 {
            0
        } else {
            let available_from_offset = num_scroll_cols.saturating_sub(self.col_offset);
            if self.col_width_override > 0 {
                let cw = self.col_width_override.max(8);
                let fits = remaining_w / (cw + 1); // +1 for separator
                fits.max(1).min(available_from_offset)
            } else {
                // Auto: fit as many as we can, each getting equal share
                available_from_offset.min(remaining_w / 10).max(1)
            }
        };

        let scroll_end = (scroll_start + visible_scroll).min(cols.len());
        let scrollable_cols = if scroll_start < cols.len() {
            &cols[scroll_start..scroll_end]
        } else {
            &[]
        };
        self.last_visible_scroll_cols = scrollable_cols.len();

        // Calculate actual column width to fill the remaining space
        let actual_col_width = if scrollable_cols.is_empty() {
            remaining_w
        } else {
            let seps_total = scrollable_cols.len(); // one sep char per col
            remaining_w.saturating_sub(seps_total) / scrollable_cols.len()
        };
        let actual_col_width = actual_col_width.max(8);

        // Last column gets extra space to fill any remainder
        let used = scrollable_cols.len() * actual_col_width + scrollable_cols.len();
        let last_col_extra = remaining_w.saturating_sub(used);

        let header_bg = Color::Rgb(50, 40, 30);
        let header_style = Style::default()
            .fg(Color::Rgb(180, 160, 130))
            .bg(header_bg)
            .add_modifier(Modifier::BOLD);
        let sort_header = Style::default()
            .fg(Color::Cyan)
            .bg(header_bg)
            .add_modifier(Modifier::BOLD);

        let has_more_right = scroll_end < cols.len();
        let _has_more_left = scroll_start > 1;

        let sort_arrow = |idx: usize| -> &str {
            if self.sort_col_idx == Some(idx) {
                if self.sort_ascending { " ▲" } else { " ▼" }
            } else {
                ""
            }
        };
        let col_hdr_style = |idx: usize| -> Style {
            if self.sort_col_idx == Some(idx) {
                sort_header
            } else {
                header_style
            }
        };

        // Header row
        let sticky_label = format!(
            "{}{}",
            truncate_str(sticky_col, sticky_width.saturating_sub(3)),
            sort_arrow(0)
        );
        let mut hdr_spans = vec![
            Span::styled(
                format!(" {:<width$} ", sticky_label, width = sticky_width),
                col_hdr_style(0),
            ),
            Span::styled(sep, dim),
        ];

        for (ci, col) in scrollable_cols.iter().enumerate() {
            let global_idx = scroll_start + ci;
            let cw = if ci == scrollable_cols.len() - 1 {
                actual_col_width + last_col_extra
            } else {
                actual_col_width
            };
            let label = format!(
                "{}{}",
                truncate_str(col, cw.saturating_sub(3)),
                sort_arrow(global_idx)
            );
            hdr_spans.push(Span::styled(
                format!(" {:<width$}", label, width = cw),
                col_hdr_style(global_idx),
            ));
            if ci < scrollable_cols.len() - 1 {
                hdr_spans.push(Span::styled(sep, dim));
            }
        }

        if has_more_right {
            hdr_spans.push(Span::styled(
                "›",
                Style::default().fg(Color::DarkGray).bg(header_bg),
            ));
        }

        // Fill rest of header line with header bg
        let hdr_line = Line::from(hdr_spans);

        // Header separator line
        let mut sep_line_spans = vec![
            Span::styled(format!(" {}", "─".repeat(sticky_width + 1)), row_line_style),
            Span::styled("┼", row_line_style),
        ];
        for (ci, _) in scrollable_cols.iter().enumerate() {
            let cw = if ci == scrollable_cols.len() - 1 {
                actual_col_width + last_col_extra + 1
            } else {
                actual_col_width + 1
            };
            sep_line_spans.push(Span::styled("─".repeat(cw), row_line_style));
            if ci < scrollable_cols.len() - 1 {
                sep_line_spans.push(Span::styled("┼", row_line_style));
            }
        }
        let sep_line = Line::from(sep_line_spans);

        // Render sticky header (2 rows: header + separator)
        let header_area = Rect {
            x: list_area.x,
            y: list_area.y,
            width: list_area.width,
            height: 1,
        };
        let sep_area = Rect {
            x: list_area.x,
            y: list_area.y + 1,
            width: list_area.width,
            height: 1,
        };
        // Fill header bg across full width
        let bg_fill = Paragraph::new("").style(Style::default().bg(header_bg));
        frame.render_widget(bg_fill, header_area);
        frame.render_widget(Paragraph::new(hdr_line), header_area);
        frame.render_widget(Paragraph::new(sep_line), sep_area);

        // Data area starts below header
        let data_area = Rect {
            x: list_area.x,
            y: list_area.y + 2,
            width: list_area.width,
            height: list_area.height.saturating_sub(2),
        };

        let mut items_list: Vec<ListItem> = Vec::new();

        for (i, item) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let null_style = if is_selected {
                style
            } else {
                Style::default().fg(Color::Rgb(80, 80, 80))
            };
            let sep_style = if is_selected { style } else { dim };

            let sticky_val = item
                .attributes
                .get(sticky_col.as_str())
                .map(|s| s.as_str())
                .unwrap_or("");
            let sticky_display = truncate_str(sticky_val, sticky_width);
            let sticky_cell_style = if sticky_val == "null" || sticky_val.is_empty() {
                null_style
            } else {
                style
            };

            let mut row_spans = vec![
                Span::styled(
                    format!(" {:<width$} ", sticky_display, width = sticky_width),
                    sticky_cell_style,
                ),
                Span::styled(sep, sep_style),
            ];

            for (ci, col) in scrollable_cols.iter().enumerate() {
                let cw = if ci == scrollable_cols.len() - 1 {
                    actual_col_width + last_col_extra
                } else {
                    actual_col_width
                };
                let val = item
                    .attributes
                    .get(col.as_str())
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let display = truncate_str(val, cw);
                let cell_style = if val == "null" || val.is_empty() {
                    null_style
                } else {
                    style
                };
                row_spans.push(Span::styled(
                    format!(" {:<width$}", display, width = cw),
                    cell_style,
                ));
                if ci < scrollable_cols.len() - 1 {
                    row_spans.push(Span::styled(sep, sep_style));
                }
            }

            // Row separator (thin line)
            let mut rsep_spans = vec![
                Span::styled(format!(" {}", "─".repeat(sticky_width + 1)), row_line_style),
                Span::styled("┼", row_line_style),
            ];
            for (ci, _) in scrollable_cols.iter().enumerate() {
                let cw = if ci == scrollable_cols.len() - 1 {
                    actual_col_width + last_col_extra + 1
                } else {
                    actual_col_width + 1
                };
                rsep_spans.push(Span::styled("─".repeat(cw), row_line_style));
                if ci < scrollable_cols.len() - 1 {
                    rsep_spans.push(Span::styled("┼", row_line_style));
                }
            }

            // Combine data row + separator into one ListItem so scrolling never splits them
            items_list.push(ListItem::new(vec![
                Line::from(row_spans),
                Line::from(rsep_spans),
            ]));
        }

        let list = List::new(items_list);
        self.items_list_state.select(Some(self.selected));
        frame.render_stateful_widget(list, data_area, &mut self.items_list_state);

        let clear = Paragraph::new(Span::styled(
            " ".repeat(info_area.width as usize),
            Style::default(),
        ));
        frame.render_widget(clear, info_area);

        let orange = Style::default().fg(Color::Rgb(255, 165, 0));
        let sep_style = Style::default().fg(Color::Rgb(100, 100, 100));

        let mut info_spans = Vec::new();

        if let Some(ref detail) = self.table_detail {
            let (pk, pk_type, sk, sk_type, item_count) =
                if let Some(ref idx_name) = self.active_index {
                    if let Some(idx) = detail.indexes.iter().find(|i| &i.name == idx_name) {
                        (
                            idx.partition_key.as_str(),
                            idx.partition_key_type.as_str(),
                            idx.sort_key.as_deref(),
                            idx.sort_key_type.as_deref(),
                            idx.item_count,
                        )
                    } else {
                        (
                            detail.partition_key.as_str(),
                            detail.partition_key_type.as_str(),
                            detail.sort_key.as_deref(),
                            detail.sort_key_type.as_deref(),
                            detail.item_count,
                        )
                    }
                } else {
                    (
                        detail.partition_key.as_str(),
                        detail.partition_key_type.as_str(),
                        detail.sort_key.as_deref(),
                        detail.sort_key_type.as_deref(),
                        detail.item_count,
                    )
                };

            info_spans.push(Span::styled(format!(" {}:{}", pk, pk_type), orange));
            if let Some(sk_name) = sk {
                let skt = sk_type.unwrap_or("S");
                info_spans.push(Span::styled(format!(" {}:{}", sk_name, skt), orange));
            }
            info_spans.push(Span::styled(" │ ", sep_style));
            info_spans.push(Span::styled(&detail.billing_mode, orange));
            info_spans.push(Span::styled(" │ ", sep_style));
            info_spans.push(Span::styled(
                format!("~{} total", format_number(item_count)),
                orange,
            ));
        }

        if let Some(ref result) = self.items_result {
            let (page, total) = self.page_info();
            info_spans.push(Span::styled(" │ ", sep_style));
            info_spans.push(Span::styled(
                format!("{} returned", result.items.len()),
                orange,
            ));
            info_spans.push(Span::styled(" │ ", sep_style));
            info_spans.push(Span::styled(format!("Page {}/{}", page, total), orange));
            if self.has_next_page() || self.current_page > 0 {
                info_spans.push(Span::styled(" (n/N)", Style::default().fg(Color::DarkGray)));
            }
        }

        if let Some(ref summary) = self.query_summary {
            info_spans.push(Span::styled(" │ ", sep_style));
            info_spans.push(Span::styled(summary.as_str(), orange));
        }

        let info_bar = Paragraph::new(Line::from(info_spans));
        frame.render_widget(info_bar, info_area);

        if self.filtering {
            self.render_filter(frame, info_area);
        }

        if self.index_picker_open {
            self.render_index_picker(frame, list_area);
        }

        if self.query_builder_open {
            self.render_query_builder(frame, list_area);
        }
    }

    fn render_index_picker(&self, frame: &mut Frame, area: Rect) {
        let detail = match &self.table_detail {
            Some(d) => d,
            None => return,
        };

        let index_count = 1 + detail.indexes.len();
        let picker_height = (index_count as u16 + 2).min(area.height.saturating_sub(2)); // +2 for borders
        let picker_width = 50u16.min(area.width.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(picker_width)) / 2;
        let y = area.y + (area.height.saturating_sub(picker_height)) / 2;

        let picker_area = Rect {
            x,
            y,
            width: picker_width,
            height: picker_height,
        };

        frame.render_widget(ratatui::widgets::Clear, picker_area);

        let is_active = |idx: usize| -> bool {
            if idx == 0 {
                self.active_index.is_none()
            } else {
                self.active_index.as_deref() == detail.indexes.get(idx - 1).map(|i| i.name.as_str())
            }
        };

        let mut items: Vec<ListItem> = Vec::new();

        // Table (Primary)
        let pk_info = format!("{}:{}", detail.partition_key, detail.partition_key_type);
        let sk_info = detail
            .sort_key
            .as_ref()
            .map(|sk| {
                let sk_type = detail.sort_key_type.as_deref().unwrap_or("S");
                format!(" {}:{}", sk, sk_type)
            })
            .unwrap_or_default();
        let marker = if is_active(0) { "● " } else { "  " };
        let label = format!("{}Table (Primary)", marker);
        let keys = format!("{}{}", pk_info, sk_info);
        let style = if self.index_picker_selected == 0 {
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else if is_active(0) {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        let inner_w = picker_width.saturating_sub(2) as usize;
        let keys_w = inner_w.saturating_sub(label.len()).saturating_sub(1);
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {:<pad$}", label, pad = inner_w - keys_w - 1),
                style,
            ),
            Span::styled(format!("{:>width$} ", keys, width = keys_w), style),
        ])));

        // GSIs
        for (i, idx) in detail.indexes.iter().enumerate() {
            let row_idx = i + 1;
            let marker = if is_active(row_idx) { "● " } else { "  " };
            let label = format!("{}{}", marker, idx.name);
            let pk_info = format!("{}:{}", idx.partition_key, idx.partition_key_type);
            let sk_info = idx
                .sort_key
                .as_ref()
                .map(|sk| {
                    let sk_type = idx.sort_key_type.as_deref().unwrap_or("S");
                    format!(" {}:{}", sk, sk_type)
                })
                .unwrap_or_default();
            let keys = format!("{}{}", pk_info, sk_info);
            let style = if self.index_picker_selected == row_idx {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if is_active(row_idx) {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };
            let keys_w = inner_w.saturating_sub(label.len()).saturating_sub(1);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<pad$}", label, pad = inner_w - keys_w - 1),
                    style,
                ),
                Span::styled(format!("{:>width$} ", keys, width = keys_w), style),
            ])));
        }

        let block = ratatui::widgets::Block::default()
            .title(" Select Index ")
            .title_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 100)))
            .style(Style::default().bg(Color::Rgb(30, 30, 40)));

        let list = List::new(items).block(block);
        frame.render_widget(list, picker_area);
    }

    fn qb_max_field(&self) -> usize {
        let has_sk = self.qb_has_sk();
        let base = if has_sk { 4 } else { 2 };
        // Each filter has 3 fields (attr, cond, val), plus "add filter" button
        base + self.query_filters.len() * 3
    }

    fn qb_has_sk(&self) -> bool {
        self.table_detail
            .as_ref()
            .and_then(|d| {
                if let Some(ref idx) = self.active_index {
                    d.indexes
                        .iter()
                        .find(|i| &i.name == idx)
                        .and_then(|i| i.sort_key.as_ref())
                } else {
                    d.sort_key.as_ref()
                }
            })
            .is_some()
    }

    // Returns (field_type, filter_index, sub_field)
    // field_type: 0=mode, 1=pk, 2=sk_cond, 3=sk_val, 4=filter, 5=add_filter
    fn qb_field_type(&self) -> (usize, usize, usize) {
        let f = self.query_builder_field;
        let has_sk = self.qb_has_sk();
        let base = if has_sk { 4 } else { 2 };
        if f == 0 {
            return (0, 0, 0);
        }
        if f == 1 {
            return (1, 0, 0);
        }
        if has_sk && f == 2 {
            return (2, 0, 0);
        }
        if has_sk && f == 3 {
            return (3, 0, 0);
        }
        let filter_offset = f - base;
        let filter_idx = filter_offset / 3;
        let sub = filter_offset % 3;
        if filter_idx < self.query_filters.len() {
            (4, filter_idx, sub) // sub: 0=attr, 1=cond, 2=val
        } else {
            (5, 0, 0) // add filter
        }
    }

    fn handle_query_builder_key(&mut self, key: KeyEvent) -> Option<Action> {
        let max_field = self.qb_max_field();

        if self.query_builder_editing {
            let (ft, fi, sub) = self.qb_field_type();
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.query_builder_editing = false;
                }
                KeyCode::Backspace => match ft {
                    1 => {
                        self.query_pk_value.pop();
                    }
                    3 => {
                        self.query_sk_value.pop();
                    }
                    4 if sub == 0 => {
                        self.query_filters[fi].attribute.pop();
                    }
                    4 if sub == 2 => {
                        self.query_filters[fi].value.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(c) => match ft {
                    1 => self.query_pk_value.push(c),
                    3 => self.query_sk_value.push(c),
                    4 if sub == 0 => self.query_filters[fi].attribute.push(c),
                    4 if sub == 2 => self.query_filters[fi].value.push(c),
                    _ => {}
                },
                _ => {}
            }
            return Some(Action::None);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.query_builder_field < max_field {
                    self.query_builder_field += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.query_builder_field > 0 {
                    self.query_builder_field -= 1;
                }
            }
            KeyCode::Enter | KeyCode::Char('i') => {
                let (ft, fi, sub) = self.qb_field_type();
                match ft {
                    0 => self.query_mode = !self.query_mode,
                    1 | 3 => self.query_builder_editing = true,
                    2 => {
                        let conditions = ["=", "begins_with", ">", "<", ">=", "<="];
                        let cur = conditions
                            .iter()
                            .position(|c| *c == self.query_sk_condition)
                            .unwrap_or(0);
                        self.query_sk_condition =
                            conditions[(cur + 1) % conditions.len()].to_string();
                    }
                    4 if sub == 0 => self.query_builder_editing = true,
                    4 if sub == 1 => {
                        let conditions =
                            ["=", "<>", ">", "<", ">=", "<=", "begins_with", "contains"];
                        let cur = conditions
                            .iter()
                            .position(|c| *c == self.query_filters[fi].condition)
                            .unwrap_or(0);
                        self.query_filters[fi].condition =
                            conditions[(cur + 1) % conditions.len()].to_string();
                    }
                    4 if sub == 2 => self.query_builder_editing = true,
                    5 => {
                        self.query_filters.push(DdbFilter {
                            attribute: String::new(),
                            condition: "=".to_string(),
                            value: String::new(),
                        });
                    }
                    _ => {}
                }
            }
            KeyCode::Tab => {
                self.query_mode = !self.query_mode;
            }
            KeyCode::Char('a') => {
                self.query_filters.push(DdbFilter {
                    attribute: String::new(),
                    condition: "=".to_string(),
                    value: String::new(),
                });
                // Jump to the new filter's attribute field
                let base = if self.qb_has_sk() { 4 } else { 2 };
                self.query_builder_field = base + (self.query_filters.len() - 1) * 3;
                self.query_builder_editing = true;
            }
            KeyCode::Char('d') => {
                let (ft, fi, _) = self.qb_field_type();
                if ft == 4 {
                    self.query_filters.remove(fi);
                    let new_max = self.qb_max_field();
                    if self.query_builder_field > new_max {
                        self.query_builder_field = new_max;
                    }
                }
            }
            KeyCode::Char('r') => {
                self.query_builder_open = false;
                self.loading = true;
                self.all_pages.clear();
                self.current_page = 0;
                *self.items_list_state.offset_mut() = 0;
                self.items_result = None;
                self.selected = 0;
                return Some(Action::DdbRunQuery);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.query_builder_open = false;
            }
            _ => {}
        }
        Some(Action::None)
    }

    fn render_query_builder(&self, frame: &mut Frame, area: Rect) {
        let detail = match &self.table_detail {
            Some(d) => d,
            None => return,
        };

        let (pk_name, _pk_type, sk_name, _sk_type) = if let Some(ref idx_name) = self.active_index {
            if let Some(idx) = detail.indexes.iter().find(|i| &i.name == idx_name) {
                (
                    &idx.partition_key,
                    idx.partition_key_type.as_str(),
                    idx.sort_key.as_deref(),
                    idx.sort_key_type.as_deref(),
                )
            } else {
                (
                    &detail.partition_key,
                    detail.partition_key_type.as_str(),
                    detail.sort_key.as_deref(),
                    detail.sort_key_type.as_deref(),
                )
            }
        } else {
            (
                &detail.partition_key,
                detail.partition_key_type.as_str(),
                detail.sort_key.as_deref(),
                detail.sort_key_type.as_deref(),
            )
        };

        let has_sk = sk_name.is_some();
        let filter_lines = self.query_filters.len() as u16;
        // mode + pk + sk_cond + sk_val (if has_sk) + blank + "Filters:" + filters + "add" + blank + footer + 2 borders
        let content_lines: u16 = 2 + (if has_sk { 2 } else { 0 }) + 2 + filter_lines + 1 + 2 + 2;
        let popup_height = content_lines.min(area.height.saturating_sub(2));
        let popup_width: u16 = 70u16.min(area.width.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x,
            y,
            width: popup_width,
            height: popup_height,
        };
        frame.render_widget(ratatui::widgets::Clear, popup_area);

        let sel = Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let label_s = Style::default().fg(Color::DarkGray);
        let val_s = Style::default().fg(Color::Cyan);
        let edit_s = Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80));

        let iw = popup_width.saturating_sub(2) as usize;
        let mut lines: Vec<Line> = Vec::new();

        let text_display = |val: &str, editing: bool, field_idx: usize| -> (String, Style) {
            if editing && self.query_builder_field == field_idx {
                (format!("{}█", val), edit_s)
            } else if val.is_empty() {
                (
                    "(enter value)".to_string(),
                    if self.query_builder_field == field_idx {
                        sel
                    } else {
                        Style::default().fg(Color::Rgb(80, 80, 80))
                    },
                )
            } else {
                (
                    val.to_string(),
                    if self.query_builder_field == field_idx {
                        sel
                    } else {
                        val_s
                    },
                )
            }
        };

        // Mode
        let mode_label = if self.query_mode { "Query" } else { "Scan" };
        let mode_s = if self.query_builder_field == 0 {
            sel
        } else {
            val_s
        };
        lines.push(Line::from(vec![
            Span::styled(" Mode:  ", label_s),
            Span::styled(format!("{:<w$}", mode_label, w = iw - 8), mode_s),
        ]));

        // PK
        let (pk_disp, pk_s) = text_display(&self.query_pk_value, self.query_builder_editing, 1);
        let pk_label = format!(" {} (pk): ", pk_name);
        lines.push(Line::from(vec![
            Span::styled(&pk_label, label_s),
            Span::styled(
                format!("{:<w$}", pk_disp, w = iw.saturating_sub(pk_label.len())),
                pk_s,
            ),
        ]));

        // SK
        if has_sk {
            let sk_n = sk_name.unwrap_or("sk");

            let cond_s = if self.query_builder_field == 2 {
                sel
            } else {
                val_s
            };
            let cond_label_len = sk_n.len() + 13; // " {} condition: "
            lines.push(Line::from(vec![
                Span::styled(format!(" {} condition: ", sk_n), label_s),
                Span::styled(
                    format!(
                        "{:<w$}",
                        self.query_sk_condition,
                        w = iw.saturating_sub(cond_label_len)
                    ),
                    cond_s,
                ),
            ]));

            let (sk_disp, sk_s) = text_display(&self.query_sk_value, self.query_builder_editing, 3);
            let sk_label_len = sk_n.len() + 7; // " {} (sk): "
            lines.push(Line::from(vec![
                Span::styled(format!(" {} (sk): ", sk_n), label_s),
                Span::styled(
                    format!("{:<w$}", sk_disp, w = iw.saturating_sub(sk_label_len)),
                    sk_s,
                ),
            ]));
        }

        // Filters section
        let base = if has_sk { 4 } else { 2 };
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(" Filters:", label_s)));

        for (fi, filter) in self.query_filters.iter().enumerate() {
            let attr_idx = base + fi * 3;
            let cond_idx = attr_idx + 1;
            let val_idx = attr_idx + 2;

            let (attr_disp, attr_s) =
                text_display(&filter.attribute, self.query_builder_editing, attr_idx);
            let attr_disp = if filter.attribute.is_empty()
                && !(self.query_builder_editing && self.query_builder_field == attr_idx)
            {
                "(attribute)".to_string()
            } else {
                attr_disp
            };
            let cond_s = if self.query_builder_field == cond_idx {
                sel
            } else {
                val_s
            };
            let (val_disp, fval_s) =
                text_display(&filter.value, self.query_builder_editing, val_idx);

            let del_s =
                if self.query_builder_field >= attr_idx && self.query_builder_field <= val_idx {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Rgb(60, 60, 60))
                };

            lines.push(Line::from(vec![
                Span::styled("  ", label_s),
                Span::styled(format!("{:<16}", attr_disp), attr_s),
                Span::styled(" ", label_s),
                Span::styled(format!("{:<13}", filter.condition), cond_s),
                Span::styled(" ", label_s),
                Span::styled(
                    format!("{:<w$}", val_disp, w = iw.saturating_sub(37)),
                    fval_s,
                ),
                Span::styled(" [d]", del_s),
            ]));
        }

        // Add filter
        let add_idx = base + self.query_filters.len() * 3;
        let add_s = if self.query_builder_field == add_idx {
            sel
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(vec![Span::styled("  + Add filter", add_s)]));

        // Footer
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                " r",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" run  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "a",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" add filter  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "d",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" del filter  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" mode", Style::default().fg(Color::DarkGray)),
        ]));

        let block = ratatui::widgets::Block::default()
            .title(" Query Builder ")
            .title_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(100, 100, 100)))
            .style(Style::default().bg(Color::Rgb(30, 30, 40)));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
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

        let label_style = Style::default().fg(Color::Yellow);
        let value_style = Style::default().fg(Color::White);
        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);

        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled("Item Detail", header_style)),
            Line::from(""),
        ];

        if let Some(ref detail) = self.table_detail {
            lines.push(detail_line("Table", &detail.name, label_style, value_style));
            lines.push(detail_line(
                "Status",
                &detail.status,
                label_style,
                value_style,
            ));
            let idx_name = self.active_index.as_deref().unwrap_or("Primary");
            lines.push(detail_line("Index", idx_name, label_style, value_style));
            let pk_info = format!("{} ({})", detail.partition_key, detail.partition_key_type);
            lines.push(detail_line(
                "Partition Key",
                &pk_info,
                label_style,
                value_style,
            ));
            if let Some(ref sk) = detail.sort_key {
                let sk_type = detail.sort_key_type.as_deref().unwrap_or("S");
                let sk_info = format!("{} ({})", sk, sk_type);
                lines.push(detail_line("Sort Key", &sk_info, label_style, value_style));
            }
            lines.push(detail_line(
                "Billing",
                &detail.billing_mode,
                label_style,
                value_style,
            ));
            lines.push(detail_line(
                "Total Items",
                &format_number(detail.item_count),
                label_style,
                value_style,
            ));
            lines.push(detail_line(
                "Table Size",
                &format_size(detail.size_bytes),
                label_style,
                value_style,
            ));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("JSON", header_style)));
        lines.push(Line::from(""));

        if let Some(ref json) = self.detail_json {
            lines.extend(crate::ui::highlight_json(json));
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
    Line::from(vec![
        Span::styled(format!("{:<20}", label), ls),
        Span::styled(value.to_string(), vs),
    ])
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max > 3 {
        let truncated: String = s.chars().take(max - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max).collect()
    }
}

fn format_number(n: i64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

fn smart_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    // Empty/null always sorts last
    if a.is_empty() && b.is_empty() {
        return std::cmp::Ordering::Equal;
    }
    if a.is_empty() || a == "null" {
        return std::cmp::Ordering::Greater;
    }
    if b.is_empty() || b == "null" {
        return std::cmp::Ordering::Less;
    }

    // Try numeric comparison
    if let (Ok(na), Ok(nb)) = (a.parse::<f64>(), b.parse::<f64>()) {
        return na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
    }

    // Try date-like (ISO format strings compare lexicographically correctly)
    // Booleans
    if let (Ok(ba), Ok(bb)) = (a.parse::<bool>(), b.parse::<bool>()) {
        return ba.cmp(&bb);
    }

    // Fall back to case-insensitive string comparison
    a.to_lowercase().cmp(&b.to_lowercase())
}
