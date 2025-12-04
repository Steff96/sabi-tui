//! Sabi-TUI: A terminal-based AI agent implementing the ReAct pattern

#![allow(dead_code)]

mod ai_client;
mod app;
mod config;
mod event;
mod executor;
mod gemini;
mod mcp;
mod message;
mod onboarding;
mod openai;
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

use ai_client::AIClient;
use app::{App, InputResult};
use config::Config;
use event::{Event, EventHandler};
use executor::{CommandExecutor, DangerousCommandDetector, InteractiveCommandDetector};
use gemini::SYSTEM_PROMPT;
use mcp::McpClient;
use message::Message;
use state::StateEvent;
use tool_call::ParsedResponse;

/// Tick rate for UI updates (100ms = 10 FPS)
const TICK_RATE: Duration = Duration::from_millis(100);

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!("sabi - AI-powered terminal assistant\n");
    println!("Usage:");
    println!("  sabi              Start interactive TUI");
    println!("  sabi -q 'prompt'  Quick query (text response only)");
    println!("  sabi -x 'prompt'  Execute command from prompt");
    println!("  sabi mcp <cmd>    Manage MCP servers\n");
    println!("Options:");
    println!("  -q, --query      Quick mode: get text response");
    println!("  -x, --exec       Execute mode: run command");
    println!("  --safe           Safe mode: show commands but don't execute");
    println!("  -v, --version    Show version");
    println!("  -h, --help       Show this help message\n");
    println!("MCP Commands:");
    println!("  sabi mcp add <name> <cmd> [args]  Add MCP server");
    println!("  sabi mcp remove <name>            Remove MCP server");
    println!("  sabi mcp list                     List MCP servers");
}

fn print_version() {
    println!("sabi {}", VERSION);
}

/// Check for updates from GitHub releases (non-blocking)
fn check_for_updates() {
    std::thread::spawn(|| {
        if let Ok(latest) = fetch_latest_version()
            && is_newer(&latest, VERSION)
        {
            eprintln!(
                "\n📦 Update available: {} → {}\n   Run: curl -sSL https://raw.githubusercontent.com/n4ar/sabi-tui/main/setup.sh | bash\n",
                VERSION, latest
            );
        }
    });
}

fn fetch_latest_version() -> Result<String, ()> {
    let resp = reqwest::blocking::Client::new()
        .get("https://api.github.com/repos/n4ar/sabi-tui/releases/latest")
        .header("User-Agent", format!("sabi-tui/{}", VERSION))
        .timeout(Duration::from_secs(3))
        .send()
        .map_err(|_| ())?;

    let json: serde_json::Value = resp.json().map_err(|_| ())?;
    json["tag_name"]
        .as_str()
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or(())
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    parse(latest) > parse(current)
}

/// Get system context for AI
fn get_system_context() -> String {
    let time = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".into());
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());

    let (os_name, os_version) = get_os_info();

    format!(
        "SYSTEM CONTEXT:\n\
         - Current time: {}\n\
         - User: {}\n\
         - Shell: {}\n\
         - Working directory: {}\n\
         - OS: {} {}",
        time, user, shell, cwd, os_name, os_version
    )
}

fn get_os_info() -> (String, String) {
    #[cfg(target_os = "macos")]
    {
        let version = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".into());
        ("macOS".into(), version)
    }
    #[cfg(target_os = "linux")]
    {
        let version = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                    l.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
            })
            .unwrap_or_else(|| "Linux".into());
        ("Linux".into(), version)
    }
    #[cfg(target_os = "windows")]
    {
        ("Windows".into(), "".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        ("Unknown".into(), "".into())
    }
}

/// Quick CLI mode - single query without TUI
async fn run_quick_mode(config: &Config, prompt: &str, execute: bool) -> Result<()> {
    let ai_client = AIClient::new(config)?;
    let executor = CommandExecutor::new(config);

    // Build system prompt
    let system_context = get_system_context();
    let mut system_prompt = format!("{}\n\n{}", SYSTEM_PROMPT, system_context);

    // Add MCP tools if available
    if let Ok(mcp_client) = crate::mcp::McpClient::load() {
        let _ = mcp_client.start_all();
        if let Ok(all_tools) = mcp_client.list_all_tools()
            && !all_tools.is_empty()
        {
            system_prompt.push_str("\n\n6. Call MCP external tools:\n   {\"tool\": \"mcp\", \"server\": \"<server>\", \"name\": \"<tool_name>\", \"arguments\": {<args>}}\n\nAvailable MCP tools:\n");
            for (server, tools) in &all_tools {
                for tool in tools {
                    let desc = tool.description.as_deref().unwrap_or("").lines().next().unwrap_or("");
                    let args = tool.input_schema.as_ref()
                        .and_then(|s| s.get("properties"))
                        .and_then(|p| p.as_object())
                        .map(|props| props.keys().cloned().collect::<Vec<_>>().join(", "))
                        .unwrap_or_default();
                    system_prompt.push_str(&format!(
                        "- {}/{}: {}\n  Args: {}\n  Example: {{\"tool\": \"mcp\", \"server\": \"{}\", \"name\": \"{}\", \"arguments\": {{{}}}}}\n",
                        server, tool.name, desc, args, server, tool.name,
                        if args.is_empty() { "".to_string() } else { format!("\"{}\": \"...\"", args.split(", ").next().unwrap_or("")) }
                    ));
                }
            }
        }
    }

    let messages = vec![Message::system(&system_prompt), Message::user(prompt)];

    // Get AI response
    println!("🤔 Thinking...");
    let response = ai_client.chat(&messages).await?;

    // Parse response
    match ParsedResponse::parse(&response) {
        ParsedResponse::ToolCall(tool) => {
            if tool.tool == "mcp" {
                // Handle MCP tool call
                println!("🔌 Calling MCP tool: {}/{}", tool.server, tool.name);
                if let Ok(mcp_client) = crate::mcp::McpClient::load() {
                    let _ = mcp_client.start_all();
                    match mcp_client.call_tool(&tool.server, &tool.name, tool.arguments.clone()) {
                        Ok(result) => {
                            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()));
                        }
                        Err(e) => {
                            println!("❌ MCP error: {:?}", e);
                        }
                    }
                }
            } else if execute {
                // Show confirmation dialog
                if !show_confirmation_dialog(&tool.command, &response)? {
                    println!("❌ Cancelled");
                    return Ok(());
                }

                println!("🔧 Executing...");
                let result = executor.execute_tool_async(&tool).await;

                // Get AI summary
                println!("🤖 Summarizing...");
                let user_msg = format!(
                    "Command: {}\nExit code: {}\nOutput:\n{}{}",
                    tool.command,
                    result.exit_code,
                    result.stdout,
                    if result.stderr.is_empty() {
                        String::new()
                    } else {
                        format!("\nStderr:\n{}", result.stderr)
                    }
                );
                let summary_messages = vec![
                    Message::system("Summarize this command output concisely in 1-2 sentences."),
                    Message::user(&user_msg),
                ];
                let summary = ai_client
                    .chat(&summary_messages)
                    .await
                    .unwrap_or_else(|_| "Execution complete.".into());

                // Show result TUI
                show_result_dialog(
                    &tool.command,
                    &result.stdout,
                    &result.stderr,
                    result.exit_code,
                    &summary,
                )?;

                std::process::exit(result.exit_code);
            } else {
                println!("{}", tool.command);
            }
        }
        ParsedResponse::TextResponse(text) => {
            println!("{}", text);
        }
    }

    Ok(())
}

/// Show TUI confirmation dialog for command execution
fn show_confirmation_dialog(command: &str, explanation: &str) -> Result<bool> {
    use crossterm::event::{self, Event, KeyCode};
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph, Wrap},
    };

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|f| {
            let area = f.area();

            // Center dialog
            let dialog_width = area.width.min(80);
            let dialog_height = area.height.min(20);
            let x = (area.width.saturating_sub(dialog_width)) / 2;
            let y = (area.height.saturating_sub(dialog_height)) / 2;
            let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

            // Split into sections
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Min(5),    // Explanation
                    Constraint::Length(3), // Command
                    Constraint::Length(3), // Buttons
                ])
                .split(dialog_area);

            // Title
            let title = Paragraph::new("⚠️  Confirm Command Execution")
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            // Explanation
            let explanation_text = Paragraph::new(explanation)
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" AI Explanation "),
                );
            f.render_widget(explanation_text, chunks[1]);

            // Command
            let cmd_text = Paragraph::new(command)
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(" Command "));
            f.render_widget(cmd_text, chunks[2]);

            // Buttons
            let buttons = Paragraph::new(Line::from(vec![
                Span::styled(
                    " [Enter] ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Execute  "),
                Span::styled(
                    " [Esc] ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw("Cancel"),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(buttons, chunks[3]);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => break true,
                KeyCode::Esc | KeyCode::Char('q') => break false,
                _ => {}
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(result)
}

/// Show TUI result dialog after execution
fn show_result_dialog(
    command: &str,
    stdout_out: &str,
    stderr_out: &str,
    exit_code: i32,
    summary: &str,
) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode};
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph, Wrap},
    };

    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout_handle);
    let mut terminal = Terminal::new(backend)?;

    let status_color = if exit_code == 0 {
        Color::Green
    } else {
        Color::Red
    };
    let status_icon = if exit_code == 0 { "✅" } else { "❌" };

    loop {
        terminal.draw(|f| {
            let area = f.area();

            let dialog_width = area.width.min(90);
            let dialog_height = area.height.min(25);
            let x = (area.width.saturating_sub(dialog_width)) / 2;
            let y = (area.height.saturating_sub(dialog_height)) / 2;
            let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Length(3), // Command
                    Constraint::Min(6),    // Output
                    Constraint::Length(5), // Summary
                    Constraint::Length(3), // Footer
                ])
                .split(dialog_area);

            // Title
            let title = Paragraph::new(format!(
                "{} Execution Complete (exit: {})",
                status_icon, exit_code
            ))
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            // Command
            let cmd = Paragraph::new(command)
                .style(Style::default().fg(Color::Cyan))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(" Command "));
            f.render_widget(cmd, chunks[1]);

            // Output
            let output = if stderr_out.is_empty() {
                stdout_out.to_string()
            } else {
                format!("{}\n--- stderr ---\n{}", stdout_out, stderr_out)
            };
            let output_widget = Paragraph::new(output)
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title(" Output "));
            f.render_widget(output_widget, chunks[2]);

            // AI Summary
            let summary_widget = Paragraph::new(summary)
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" 🤖 AI Summary "),
                );
            f.render_widget(summary_widget, chunks[3]);

            // Footer
            let footer = Paragraph::new(Line::from(vec![
                Span::styled(
                    " [Enter/Esc] ",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Close"),
            ]))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[4]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => break,
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Check for updates in background
    check_for_updates();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|a| a == "--version" || a == "-v") {
        print_version();
        return Ok(());
    }

    // Handle MCP commands: sabi mcp <subcommand>
    if args.get(1).map(|s| s.as_str()) == Some("mcp") {
        let mcp_args: Vec<String> = args[2..].to_vec();
        if let Err(e) = mcp::handle_mcp_command(&mcp_args) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

    let mut config = Config::load().context("Failed to load configuration")?;

    // CLI flag overrides config
    if args.iter().any(|a| a == "--safe") {
        config.safe_mode = true;
    }

    // Run onboarding if no API key configured
    if !config.has_api_key() {
        config = onboarding::run_onboarding().context("Onboarding failed")?;
        // Create default mcp.toml during onboarding
        let _ = mcp::McpConfig::create_default_if_missing();
    }

    // Quick mode: -q "prompt" (text only) or -x "prompt" (execute)
    let query_mode = args.iter().position(|a| a == "-q" || a == "--query");
    let exec_mode = args.iter().position(|a| a == "-x" || a == "--exec");

    if let Some(pos) = query_mode.or(exec_mode) {
        let execute = exec_mode.is_some();
        let prompt = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("");

        if prompt.is_empty() {
            eprintln!("Error: No prompt provided");
            eprintln!("Usage: sabi -q 'prompt' or sabi -x 'prompt'");
            std::process::exit(1);
        }

        return run_quick_mode(&config, prompt, execute).await;
    }

    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    let mut app = App::new(config.clone());
    let mut events = EventHandler::new(TICK_RATE);

    // Start MCP servers if configured
    let mcp_servers = app.start_mcp_servers();

    // Gather system context
    let system_context = get_system_context();

    // Build system prompt (include Python tool if available)
    let mut system_prompt = if app.python_available {
        format!(
            "{}\n\n5. Run Python code:\n   {{\"tool\": \"run_python\", \"code\": \"<python code>\"}}\n\nEXAMPLE:\n- \"calculate 2^100\" → {{\"tool\": \"run_python\", \"code\": \"print(2**100)\"}}\n\n{}",
            SYSTEM_PROMPT, system_context
        )
    } else {
        format!("{}\n\n{}", SYSTEM_PROMPT, system_context)
    };

    // Add MCP tools to system prompt
    let mcp_tools_prompt = app.get_mcp_tools_prompt();
    if !mcp_tools_prompt.is_empty() {
        system_prompt.push_str(&mcp_tools_prompt);
    }

    app.add_message(Message::system(&system_prompt));

    // Show MCP status if servers started
    if !mcp_servers.is_empty() {
        app.add_message(Message::model(format!(
            "🔌 MCP servers started: {}",
            mcp_servers.join(", ")
        )));
    }

    // Auto-load previous session
    app.auto_load();

    let ai_client = AIClient::new(&config).ok();
    let detector = DangerousCommandDetector::new(&config.dangerous_patterns);
    let interactive_detector = InteractiveCommandDetector::new();

    let result = run_loop(
        &mut terminal,
        &mut app,
        &mut events,
        ai_client,
        detector,
        interactive_detector,
    )
    .await;

    // Auto-save session before exit
    app.auto_save();

    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("Failed to leave alternate screen")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App<'_>,
    events: &mut EventHandler,
    mut ai_client: Option<AIClient>,
    detector: DangerousCommandDetector,
    interactive_detector: InteractiveCommandDetector,
) -> Result<()> {
    let tx = events.sender();

    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        if let Some(event) = events.next().await {
            match event {
                Event::Key(key) => {
                    let result = app.handle_key_event(key);

                    // Handle command cancellation
                    if result == InputResult::CancelCommand {
                        app.add_message(Message::system("⚠️ Command cancelled"));
                        app.transition(StateEvent::AnalysisComplete);
                        continue;
                    }

                    // Handle /model command
                    if let InputResult::FetchModels(model_arg) = result.clone() {
                        if let Some(ref client) = ai_client {
                            let client_clone = client.clone();
                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let models = client_clone.list_models().await;
                                let _ = tx_clone.send(Event::ModelsResponse(models, model_arg));
                            });
                        } else {
                            app.add_message(Message::system("API key not configured"));
                        }
                        continue;
                    }

                    // 12.1: Input → Thinking transition
                    if result == InputResult::SubmitQuery {
                        if let Some(ref client) = ai_client {
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
                    if result == InputResult::ExecuteCommand
                        && let Some(ref tool) = app.current_tool
                    {
                        // Safe mode: don't execute, just show what would run
                        if app.config.safe_mode {
                            let desc = match tool.tool.as_str() {
                                "run_cmd" => format!("Would run: {}", tool.command),
                                "run_python" => format!("Would run Python:\n{}", tool.code),
                                "read_file" => format!("Would read: {}", tool.path),
                                "write_file" => format!(
                                    "Would write {} bytes to: {}",
                                    tool.content.len(),
                                    tool.path
                                ),
                                "search" => {
                                    format!("Would search '{}' in {}", tool.pattern, tool.directory)
                                }
                                "mcp" => {
                                    format!("Would call MCP: {}/{}", tool.server, tool.name)
                                }
                                _ => format!("Would execute: {:?}", tool),
                            };
                            app.add_message(Message::system(format!("🔒 [SAFE MODE] {}", desc)));
                            app.transition(StateEvent::AnalysisComplete);
                        } else if tool.is_mcp() {
                            // Execute MCP tool asynchronously
                            if app.mcp_client.is_some() {
                                let server = tool.server.clone();
                                let name = tool.name.clone();
                                let arguments = tool.arguments.clone();
                                let tx_clone = tx.clone();
                                
                                // Clone what we need for the blocking task
                                let mcp = McpClient::load();
                                
                                tokio::task::spawn_blocking(move || {
                                    let result = match mcp {
                                        Ok(client) => {
                                            // Start the server if needed
                                            let _ = client.start_server(&server);
                                            client.call_tool(&server, &name, arguments)
                                                .map_err(|e| e.to_string())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    };
                                    let _ = tx_clone.send(Event::McpResult(result, server, name));
                                });
                                // State already transitioned to Executing by handle_key_event
                            } else {
                                app.add_message(Message::system("❌ MCP client not available"));
                                app.transition(StateEvent::AnalysisComplete);
                            }
                        } else {
                            let tool = tool.clone();
                            let exec = CommandExecutor::new(&app.config);
                            let tx_clone = tx.clone();
                            let handle = tokio::spawn(async move {
                                let result = exec.execute_tool_async(&tool).await;
                                let _ = tx_clone.send(Event::CommandComplete(result));
                            });
                            app.running_task = Some(handle);
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
                                ParsedResponse::ToolCall(tc) => {
                                    // Format display text based on tool type
                                    let display = match tc.tool.as_str() {
                                        "run_cmd" => tc.command.clone(),
                                        "run_python" => format!("python:\n{}", tc.code),
                                        "read_file" => format!("read_file: {}", tc.path),
                                        "write_file" => format!(
                                            "write_file: {} ({} bytes)",
                                            tc.path,
                                            tc.content.len()
                                        ),
                                        "search" => format!(
                                            "search: {} in {}",
                                            tc.pattern,
                                            if tc.directory.is_empty() {
                                                "."
                                            } else {
                                                &tc.directory
                                            }
                                        ),
                                        "mcp" => format!(
                                            "mcp: {}/{}\n{}",
                                            tc.server,
                                            tc.name,
                                            serde_json::to_string_pretty(&tc.arguments).unwrap_or_default()
                                        ),
                                        _ => format!("{:?}", tc),
                                    };

                                    // Check for interactive commands
                                    if tc.is_run_cmd()
                                        && interactive_detector.is_interactive(&tc.command)
                                    {
                                        let suggestion =
                                            interactive_detector.suggestion(&tc.command).unwrap_or(
                                                "This command requires an interactive terminal",
                                            );
                                        app.add_message(Message::model(format!(
                                            "⚠️ Cannot run interactive command: `{}`\n{}",
                                            tc.command, suggestion
                                        )));
                                        app.transition(StateEvent::TextResponseReceived);
                                        continue;
                                    }

                                    // Check Python availability
                                    if tc.tool == "run_python" && !app.python_available {
                                        app.add_message(Message::model(
                                            "⚠️ Python is not available on this system.\nPlease install Python 3 to use this feature."
                                        ));
                                        app.transition(StateEvent::TextResponseReceived);
                                        continue;
                                    }

                                    app.set_action_text(&display);
                                    app.current_tool = Some((*tc).clone());

                                    // Check for dangerous operations
                                    app.dangerous_command_detected = tc.is_destructive()
                                        || (tc.is_run_cmd() && detector.is_dangerous(&tc.command));

                                    // Block unknown tools entirely
                                    if !tc.is_allowed_tool() {
                                        app.add_message(Message::system(format!(
                                            "⛔ Blocked unknown tool: '{}'\nAllowed: run_cmd, read_file, write_file, search, run_python",
                                            tc.tool
                                        )));
                                        app.transition(StateEvent::TextResponseReceived);
                                        continue;
                                    }

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
                    app.running_task = None;
                    app.execution_output = if result.success {
                        result.stdout.clone()
                    } else {
                        format!("{}\n{}", result.stdout, result.stderr)
                    };

                    let tool_desc = app
                        .current_tool
                        .as_ref()
                        .map(|t| {
                            format!(
                                "{}: {}",
                                t.tool,
                                if t.tool == "run_cmd" {
                                    &t.command
                                } else {
                                    &t.path
                                }
                            )
                        })
                        .unwrap_or_default();

                    let feedback = format!(
                        "Tool: {}\nExit code: {}\nOutput:\n{}",
                        tool_desc, result.exit_code, &app.execution_output
                    );
                    app.add_message(Message::user(&feedback));
                    app.transition(StateEvent::CommandComplete);

                    // Send to AI for analysis
                    if let Some(ref client) = ai_client {
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

                Event::CommandCancelled => {
                    // Task was cancelled, already handled in key event
                }

                Event::ModelsResponse(result, model_arg) => {
                    match result {
                        Ok(models) => {
                            if let Some(model_name) = model_arg {
                                // Switch to specified model
                                if let Some(matched) =
                                    models.iter().find(|m| m.contains(&model_name))
                                {
                                    if let Some(ref mut client) = ai_client {
                                        client.set_model(matched.clone());
                                        app.add_message(Message::system(format!(
                                            "✓ Switched to: {}",
                                            matched
                                        )));
                                    }
                                } else {
                                    app.add_message(Message::system(format!(
                                        "✗ Model '{}' not found",
                                        model_name
                                    )));
                                }
                            } else {
                                // List all models
                                let current =
                                    ai_client.as_ref().map(|c| c.model()).unwrap_or("unknown");
                                let list = models
                                    .iter()
                                    .map(|m| {
                                        if m == current {
                                            format!("→ {}", m)
                                        } else {
                                            format!("  {}", m)
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                app.add_message(Message::system(format!(
                                    "Available models:\n{}\n\nUse /model <name> to switch",
                                    list
                                )));
                            }
                        }
                        Err(e) => {
                            app.add_message(Message::system(format!(
                                "✗ Failed to fetch models: {}",
                                e
                            )));
                        }
                    }
                }

                Event::McpResult(result, server, tool_name) => {
                    app.running_task = None;
                    match result {
                        Ok(value) => {
                            let output = serde_json::to_string_pretty(&value).unwrap_or_default();
                            let feedback = format!(
                                "Tool: mcp/{}/{}\nOutput:\n{}",
                                server, tool_name, output
                            );
                            app.add_message(Message::user(&feedback));
                            app.transition(StateEvent::CommandComplete);

                            // Send to AI for analysis
                            if let Some(ref client) = ai_client {
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
                        Err(e) => {
                            app.add_message(Message::system(format!("❌ MCP error: {}", e)));
                            app.transition(StateEvent::AnalysisComplete);
                        }
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
