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
    event::{self as crossterm_event, Event as CrosstermEvent, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use tui_textarea::TextArea;

use app::{App, InputResult};
use config::Config;
use event::{Event, EventHandler};
use executor::{CommandExecutor, DangerousCommandDetector};
use gemini::{GeminiClient, SYSTEM_PROMPT};
use message::Message;
use state::StateEvent;
use tool_call::ParsedResponse;

/// Tick rate for UI updates (100ms = 10 FPS)
const TICK_RATE: Duration = Duration::from_millis(100);

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = Config::load().context("Failed to load configuration")?;

    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Check if API key is configured, if not show setup screen
    if !config.has_api_key() {
        match run_setup(&mut terminal)? {
            Some(api_key) => {
                config.api_key = api_key;
                let _ = config.save(); // Save for future use
            }
            None => {
                // User cancelled setup
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                terminal.show_cursor()?;
                return Ok(());
            }
        }
    }

    let mut app = App::new(config.clone());
    let mut events = EventHandler::new(TICK_RATE);
    
    // Initialize with system prompt
    app.add_message(Message::system(SYSTEM_PROMPT));

    let gemini = GeminiClient::new(&config).ok();
    let detector = DangerousCommandDetector::new(&config.dangerous_patterns);

    let result = run_loop(&mut terminal, &mut app, &mut events, gemini, detector).await;

    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    result
}

/// Run the API key setup screen
fn run_setup(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<Option<String>> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Paste your API key here...");
    
    loop {
        terminal.draw(|frame| render_setup(frame, &textarea))?;
        
        if crossterm_event::poll(Duration::from_millis(100))? {
            if let CrosstermEvent::Key(key) = crossterm_event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        let api_key = textarea.lines().join("").trim().to_string();
                        if !api_key.is_empty() {
                            return Ok(Some(api_key));
                        }
                    }
                    KeyCode::Esc => return Ok(None),
                    _ => { textarea.input(key); }
                }
            }
        }
    }
}

/// Render the setup screen
fn render_setup(frame: &mut Frame, textarea: &TextArea) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .margin(2)
        .split(frame.area());

    let welcome = Paragraph::new(
        "Welcome to agent-rs!\n\n\
         To get started, you need a Gemini API key.\n\
         Get one at: https://aistudio.google.com/apikey"
    )
    .style(Style::default().fg(Color::Cyan))
    .block(Block::default().borders(Borders::ALL).title(" Setup "));
    frame.render_widget(welcome, chunks[0]);

    let mut input = textarea.clone();
    input.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title(" API Key ")
            .border_style(Style::default().fg(Color::Green))
    );
    frame.render_widget(&input, chunks[1]);

    let help = Paragraph::new("Enter: Save and continue | Esc: Quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[2]);
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App<'_>,
    events: &mut EventHandler,
    gemini: Option<GeminiClient>,
    detector: DangerousCommandDetector,
) -> Result<()> {
    let tx = events.sender();

    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        if let Some(event) = events.next().await {
            match event {
                Event::Key(key) => {
                    let result = app.handle_key_event(key);
                    
                    // 12.1: Input → Thinking transition
                    if result == InputResult::SubmitQuery {
                        if let Some(ref client) = gemini {
                            let messages = app.messages.clone();
                            let client_clone = client.clone();
                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let response = client_clone.chat(&messages).await;
                                let _ = tx_clone.send(Event::ApiResponse(response));
                            });
                        } else {
                            app.set_error("API key not configured");
                            app.transition(StateEvent::ApiError);
                        }
                    }
                    
                    // 12.4: ReviewAction → Executing transition
                    if result == InputResult::ExecuteCommand {
                        if let Some(ref cmd) = app.current_command {
                            let cmd = cmd.clone();
                            let exec = CommandExecutor::new(&app.config);
                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let result = exec.execute(&cmd);
                                let _ = tx_clone.send(Event::CommandComplete(result));
                            });
                        }
                    }
                }
                Event::Tick => {
                    app.tick_spinner();
                }
                Event::Resize(_, _) => {}
                
                // 12.2: Thinking → ReviewAction/Input transition
                Event::ApiResponse(response) => {
                    match response {
                        Ok(text) => {
                            app.add_message(Message::model(&text));
                            
                            match ParsedResponse::parse(&text) {
                                ParsedResponse::ToolCall(tc) if tc.is_run_cmd() => {
                                    app.set_action_text(&tc.command);
                                    app.dangerous_command_detected = detector.is_dangerous(&tc.command);
                                    app.transition(StateEvent::ToolCallReceived);
                                }
                                _ => {
                                    app.transition(StateEvent::TextResponseReceived);
                                }
                            }
                        }
                        Err(e) => {
                            app.set_error(e.to_string());
                            app.transition(StateEvent::ApiError);
                        }
                    }
                }
                
                // 12.5: Executing → Finalizing → Input loop
                Event::CommandComplete(result) => {
                    app.execution_output = if result.success {
                        result.stdout.clone()
                    } else {
                        format!("{}\n{}", result.stdout, result.stderr)
                    };
                    
                    let feedback = format!(
                        "Command: {}\nExit code: {}\nOutput:\n{}",
                        app.current_command.as_deref().unwrap_or(""),
                        result.exit_code,
                        &app.execution_output
                    );
                    app.add_message(Message::user(&feedback));
                    app.transition(StateEvent::CommandComplete);
                    
                    // Send to AI for analysis
                    if let Some(ref client) = gemini {
                        let messages = app.messages.clone();
                        let client_clone = client.clone();
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            let response = client_clone.chat(&messages).await;
                            let _ = tx_clone.send(Event::ApiResponse(response));
                        });
                    } else {
                        app.transition(StateEvent::AnalysisComplete);
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
