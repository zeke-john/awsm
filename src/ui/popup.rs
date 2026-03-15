use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

pub fn render_help(frame: &mut Frame, scroll: &mut u16) {
    let full = frame.area();
    let width = 48u16.min(full.width);
    let height = 24u16.min(full.height.saturating_sub(2));

    let area = Rect {
        x: full.width.saturating_sub(width).saturating_sub(1),
        y: full.height.saturating_sub(height).saturating_sub(1),
        width,
        height,
    };

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" ? ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let arrow_style = Style::default().fg(Color::Rgb(100, 100, 100));
    let desc_style = Style::default().fg(Color::Gray);
    let header_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(Span::styled(" Navigation", header_style)),
        help_line(" j / k", "up / down", key_style, arrow_style, desc_style),
        help_line(
            " gg / G",
            "top / bottom",
            key_style,
            arrow_style,
            desc_style,
        ),
        help_line(" Ctrl-d/u", "half-page", key_style, arrow_style, desc_style),
        help_line(
            " Enter",
            "select / drill in",
            key_style,
            arrow_style,
            desc_style,
        ),
        help_line(" Esc / h", "back", key_style, arrow_style, desc_style),
        help_line(" Tab", "sidebar ↔ main", key_style, arrow_style, desc_style),
        help_line(" Ctrl-b", "toggle sidebar", key_style, arrow_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" Search & Sort", header_style)),
        help_line(" /", "search / filter", key_style, arrow_style, desc_style),
        help_line(" s / S", "sort col right / left", key_style, arrow_style, desc_style),
        help_line(" x", "toggle sort asc/desc", key_style, arrow_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" Actions", header_style)),
        help_line(" d", "download file (S3)", key_style, arrow_style, desc_style),
        help_line(" r", "retry on error", key_style, arrow_style, desc_style),
        help_line(" + / -", "resize columns", key_style, arrow_style, desc_style),
        help_line(" 0", "reset column width", key_style, arrow_style, desc_style),
        Line::from(""),
        Line::from(Span::styled(" General", header_style)),
        help_line(" q", "quit", key_style, arrow_style, desc_style),
        help_line(" Ctrl-c", "quit", key_style, arrow_style, desc_style),
        help_line(" ?", "toggle help", key_style, arrow_style, desc_style),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Esc", key_style),
            Span::styled(" close", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let total_lines = lines.len() as u16;
    let inner_height = height.saturating_sub(2); // border top + bottom
    let max_scroll = total_lines.saturating_sub(inner_height);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((*scroll, 0));
    frame.render_widget(paragraph, area);
}

fn help_line<'a>(key: &'a str, desc: &'a str, ks: Style, arrow: Style, ds: Style) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {:<13}", key.trim()), ks),
        Span::styled("→  ", arrow),
        Span::styled(desc, ds),
    ])
}
