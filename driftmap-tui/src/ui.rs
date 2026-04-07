use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(f.size());

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[0]);

    draw_endpoint_list(f, app, content_chunks[0]);
    draw_endpoint_detail(f, app, content_chunks[1]);
    draw_health_bar(f, app, main_chunks[1]);
}

fn draw_health_bar(f: &mut Frame, app: &App, area: Rect) {
    let health = &app.health;
    let text = format!(
        " 📡 Packets/s: {} | 🔄 Buffer: {:.1}% | 💧 Dropped: {} | 🧵 Active Streams: {}",
        health.packets_per_sec,
        health.buffer_utilization * 100.0,
        health.dropped_packets,
        health.active_streams
    );

    let style = if health.dropped_packets > 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" System Health "));
    f.render_widget(p.style(style), area);
}

fn score_to_color(score: f32) -> Color {
    if score < 0.05 { Color::Green }
    else if score < 0.50 { Color::Yellow }
    else { Color::Red }
}

fn score_to_symbol(score: f32) -> &'static str {
    if score < 0.05 { "✓" }
    else if score < 0.50 { "⚠" }
    else { "✗" }
}

fn draw_endpoint_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.scores.iter().enumerate().map(|(i, score)| {
        let color = score_to_color(score.score);
        let symbol = score_to_symbol(score.score);
        let text = format!("{} {:6.1}%  {}", symbol, score.score * 100.0, score.endpoint);
        
        let style = if i == app.selected {
            Style::default().bg(Color::DarkGray).fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        ListItem::new(text).style(style)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Endpoints (s:Score n:Name r:Requests) "));
    f.render_widget(list, area);
}

fn draw_endpoint_detail(f: &mut Frame, app: &App, area: Rect) {
    if let Some(score) = app.scores.get(app.selected) {
        let text = format!(
            "Endpoint: {}\n\
             Drift Score: {:.1}%\n\
             Sample Count: {}\n\n\
             Breakdown:\n\
             - Status:   {:.1}%\n\
             - Schema:   {:.1}%\n\
             - Latency:  {:.1}%\n\
             - Headers:  {:.1}%", 
            score.endpoint, score.score * 100.0, score.sample_count,
            score.status_score * 100.0, score.schema_score * 100.0, 
            score.latency_score * 100.0, score.header_score * 100.0
        );

        let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Details "));
        f.render_widget(p, area);
    } else {
        let p = Paragraph::new("No endpoint selected or waiting for traffic...")
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        f.render_widget(p, area);
    }
}
