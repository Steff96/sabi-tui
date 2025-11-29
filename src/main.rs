//! agent-rs: A terminal-based AI agent implementing the ReAct pattern

mod app;
mod config;
mod event;
mod executor;
mod gemini;
mod message;
mod state;
mod tool_call;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use config::Config;
use event::{Event, EventHandler};

/// Tick rate for UI updates (100ms = 10 FPS)
const TICK_RATE: Duration = Duration::from_millis(100);

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;

    // Set up terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create app and event handler
    let mut app = App::new(config);
    let mut events = EventHandler::new(TICK_RATE);

    // Run the main loop
    let result = run_loop(&mut terminal, &mut app, &mut events).await;

    // Restore terminal
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    result
}

/// Main event loop
async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App<'_>,
    events: &mut EventHandler,
) -> Result<()> {
    loop {
        // Render UI
        terminal.draw(|frame| ui::render(frame, app))?;

        // Handle events
        if let Some(event) = events.next().await {
            match event {
                Event::Key(key) => {
                    app.handle_key_event(key);
                }
                Event::Tick => {
                    app.tick_spinner();
                }
                Event::Resize(_, _) => {
                    // Terminal will re-render on next iteration
                }
                Event::ApiResponse(_) | Event::CommandComplete(_) => {
                    // These will be handled in Task 12 (ReAct loop integration)
                }
            }
        }

        // Check if we should quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
