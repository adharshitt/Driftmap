use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(f.size());

    draw_endpoint_list(f, app, chunks[0]);
    draw_endpoint_detail(f, app, chunks[1]);
}

fn draw_endpoint_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.scores.iter().enumerate().map(|(i, score)| {
        let style = if i == app.selected {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(format!("{} - {:.1}%", score.endpoint, score.score * 100.0)).style(style)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Endpoints "));
    f.render_widget(list, area);
}

fn draw_endpoint_detail(f: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(score) = app.scores.get(app.selected) {
        format!("Endpoint: {}\nScore: {:.1}%\nRequests: {}", 
            score.endpoint, score.score * 100.0, score.sample_count)
    } else {
        "No endpoint selected".to_string()
    };

    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Details "));
    f.render_widget(p, area);
}
