use ratatui::{backend::CrosstermBackend, Terminal};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, stdout};
use std::time::Duration;
use tokio::sync::watch;

mod app;
mod ui;
mod events;

pub async fn run_tui(score_rx: watch::Receiver<Vec<driftmap_core::scorer::DriftScore>>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new(score_rx);
    let mut tick_rate = tokio::time::interval(Duration::from_millis(100));

    loop {
        tick_rate.tick().await;

        if app.score_rx.has_changed()? {
            app.scores = app.score_rx.borrow_and_update().clone();
            // TODO: Apply sorting based on app.sort_by here
        }

        terminal.draw(|f| ui::draw(f, &app))?;

        if events::handle_events(&mut app)? {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
