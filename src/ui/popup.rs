use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

pub fn render_help(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue));

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let header_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("  Navigation", header_style)),
        Line::from(""),
        help_line("  j / k", "Move down / up", key_style, desc_style),
        help_line("  Enter", "Select / drill in", key_style, desc_style),
        help_line("  Esc", "Back / close", key_style, desc_style),
        help_line("  Tab", "Toggle sidebar / main", key_style, desc_style),
        help_line("  gg / G", "Jump to top / bottom", key_style, desc_style),
        Line::from(""),
        Line::from(Span::styled("  General", header_style)),
        Line::from(""),
        help_line("  q", "Quit", key_style, desc_style),
        help_line("  Ctrl-c", "Quit", key_style, desc_style),
        help_line("  ?", "Toggle this help", key_style, desc_style),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn help_line<'a>(key: &'a str, desc: &'a str, ks: Style, ds: Style) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{:<14}", key), ks),
        Span::styled(desc, ds),
    ])
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
