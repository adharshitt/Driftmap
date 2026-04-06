use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;
use crate::app::{App, SortMode};

pub fn handle_events(app: &mut App) -> anyhow::Result<bool> {
    if event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(true), // quit
                KeyCode::Down | KeyCode::Char('j') => {
                    app.selected = (app.selected + 1).min(app.scores.len().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.selected = app.selected.saturating_sub(1);
                }
                KeyCode::Char('s') => app.sort_by = SortMode::ByScore,
                KeyCode::Char('n') => app.sort_by = SortMode::ByName,
                KeyCode::Char('r') => app.sort_by = SortMode::ByRequests,
                _ => {}
            }
        }
    }
    Ok(false)
}
