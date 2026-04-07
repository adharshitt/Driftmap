pub mod app;
pub mod events;
pub mod ui;

use ratatui::{backend::CrosstermBackend, Terminal};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::stdout;
use std::time::Duration;
use tokio::sync::watch;

pub async fn launch_terminal_dashboard(score_rx: watch::Receiver<driftmap_core::scorer::DashboardUpdate>) -> anyhow::Result<()> {
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
            let update = app.score_rx.borrow_and_update().clone();
            app.scores = update.scores;
            app.health = update.health;
            
            app.scores.sort_by(|a, b| match app.sort_by {
                app::SortMode::ByScore => b.score.partial_cmp(&a.score).unwrap(),
                app::SortMode::ByName => a.endpoint.cmp(&b.endpoint),
                app::SortMode::ByRequests => b.sample_count.cmp(&a.sample_count),
            });
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
