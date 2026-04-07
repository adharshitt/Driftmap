use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(1), // Divider
            Constraint::Min(0),    // Table
            Constraint::Length(1), // Keybindings
            Constraint::Length(1), // Divider
            Constraint::Length(3), // Input Bar
        ])
        .split(f.size());

    draw_header(f, app, chunks[0]);
    draw_divider(f, chunks[1]);
    draw_table(f, app, chunks[2]);
    draw_footer(f, chunks[3]);
    draw_divider(f, chunks[4]);
    draw_input(f, app, chunks[5]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let drifting_count = app.scores.iter().filter(|s| s.score > 0.05).count();

    let header = Line::from(vec![
        Span::styled(
            " DRIFT MAP ",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("◈ ", Style::default().fg(Color::Blue)),
        Span::styled(
            format!("v{} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" ".repeat(area.width as usize / 4), Style::default()),
        Span::styled("[~] Watching", Style::default().fg(Color::Cyan)),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(&app.target_a, Style::default().fg(Color::White)),
        Span::styled("  ->  ", Style::default().fg(Color::DarkGray)),
        Span::styled(&app.target_b, Style::default().fg(Color::White)),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} drifting", drifting_count),
            Style::default().fg(if drifting_count > 0 {
                Color::Yellow
            } else {
                Color::Green
            }),
        ),
    ]);
    f.render_widget(Paragraph::new(header), area);
}

fn draw_divider(f: &mut Frame, area: Rect) {
    let divider =
        Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(Color::DarkGray));
    f.render_widget(divider, area);
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["Endpoint", "Score", "Requests", "Status"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::DarkGray)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.scores.iter().enumerate().map(|(i, s)| {
        let (status_text, color, icon) = if s.score < 0.05 {
            ("Equivalent", Color::Green, "[+]")
        } else if s.score < 0.50 {
            ("Drifting", Color::Yellow, "[!]")
        } else {
            ("Diverged", Color::Red, "[x]")
        };

        let style = if i == app.selected {
            Style::default().bg(Color::Rgb(30, 30, 40)).fg(Color::White)
        } else {
            Style::default().fg(color)
        };

        Row::new(vec![
            Cell::from(format!("  {}", s.endpoint)),
            Cell::from(format!("{:>6.1}%", s.score * 100.0)),
            Cell::from(format!("{:>8}", s.sample_count)),
            Cell::from(format!("  {} {}", icon, status_text)),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Min(30),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(20),
        ],
    )
    .header(header)
    .block(Block::default().padding(ratatui::widgets::Padding::horizontal(1)));

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let footer = Paragraph::new(" [j/k] Navigate   [Enter] Inspect   [d] Diff   [q] Quit ")
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().padding(ratatui::widgets::Padding::horizontal(1)));
    f.render_widget(footer, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let input = Paragraph::new(format!("> {}", app.input))
        .style(Style::default().fg(Color::Cyan))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .border_type(BorderType::Rounded),
        );

    f.render_widget(input, area);

    // Cursor position
    f.set_cursor(area.x + app.input.len() as u16 + 3, area.y + 1);
}
