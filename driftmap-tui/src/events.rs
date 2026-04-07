use crate::app::{App, SortMode};
use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;

pub fn handle_events(app: &mut App) -> anyhow::Result<bool> {
    if event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') if app.input.is_empty() => return Ok(true), // quit if not typing
                KeyCode::Esc => {
                    app.input.clear();
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Down | KeyCode::Char('j') if app.input.is_empty() => {
                    app.selected = (app.selected + 1).min(app.scores.len().saturating_sub(1));
                    app.table_state.select(Some(app.selected));
                }
                KeyCode::Up | KeyCode::Char('k') if app.input.is_empty() => {
                    app.selected = app.selected.saturating_sub(1);
                    app.table_state.select(Some(app.selected));
                }
                KeyCode::Char('s') if app.input.is_empty() => app.sort_by = SortMode::ByScore,
                KeyCode::Char('n') if app.input.is_empty() => app.sort_by = SortMode::ByName,
                KeyCode::Char('r') if app.input.is_empty() => app.sort_by = SortMode::ByRequests,
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                _ => {}
            }
        }
    }
    Ok(false)
}
