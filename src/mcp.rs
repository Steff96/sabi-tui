//! MCP (Model Context Protocol) client implementation
//!
//! Manages MCP server processes and communicates via JSON-RPC over stdio.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Default timeout for MCP calls (30 seconds)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// MCP errors
#[derive(Debug, Error)]
pub enum McpError {
    #[error("Config not found")]
    ConfigNotFound,
    #[error("Config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Server not found: {0}")]
    ServerNotFound(String),
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Timeout after {0} seconds")]
    Timeout(u64),
    #[error("Server already exists: {0}")]
    ServerExists(String),
}

/// MCP configuration from ~/.sabi/mcp.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

/// MCP transport type
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

/// Single MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// URL for HTTP transport
    #[serde(default)]
    pub url: Option<String>,
    /// Headers for HTTP transport
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// JSON-RPC request
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

/// MCP Tool definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<serde_json::Value>,
}

/// Running MCP server process
struct McpProcess {
    child: Child,
    request_id: u64,
}

/// MCP Client - manages multiple MCP servers
pub struct McpClient {
    config: McpConfig,
    processes: Arc<Mutex<HashMap<String, McpProcess>>>,
    timeout: Duration,
}

impl McpConfig {
    /// Load MCP config from ~/.sabi/mcp.toml
    pub fn load() -> Result<Self, McpError> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Get config file path
    pub fn config_path() -> Result<PathBuf, McpError> {
        let home = dirs::home_dir().ok_or(McpError::ConfigNotFound)?;
        Ok(home.join(".sabi").join("mcp.toml"))
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), McpError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| McpError::ServerError(e.to_string()))?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Create default config file if not exists
    pub fn create_default_if_missing() -> Result<(), McpError> {
        let path = Self::config_path()?;
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let default_content = r#"# MCP Servers Configuration
# Add servers using: sabi mcp add <name> <command> [args...]
# Example: sabi mcp add filesystem npx -y @modelcontextprotocol/server-filesystem /home

[servers]
"#;
        std::fs::write(&path, default_content)?;
        Ok(())
    }

    /// Check if any MCP servers are configured
    pub fn has_servers(&self) -> bool {
        !self.servers.is_empty()
    }

    /// Add a new stdio server to config
    pub fn add_server(&mut self, name: &str, command: &str, args: Vec<String>) -> Result<(), McpError> {
        if self.servers.contains_key(name) {
            return Err(McpError::ServerExists(name.to_string()));
        }
        self.servers.insert(name.to_string(), McpServerConfig {
            transport: McpTransport::Stdio,
            command: command.to_string(),
            args,
            env: HashMap::new(),
            url: None,
            headers: HashMap::new(),
        });
        self.save()
    }

    /// Add a new HTTP server to config
    pub fn add_http_server(&mut self, name: &str, url: &str, headers: HashMap<String, String>) -> Result<(), McpError> {
        if self.servers.contains_key(name) {
            return Err(McpError::ServerExists(name.to_string()));
        }
        self.servers.insert(name.to_string(), McpServerConfig {
            transport: McpTransport::Http,
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
            url: Some(url.to_string()),
            headers,
        });
        self.save()
    }

    /// Set header for HTTP server
    pub fn set_header(&mut self, name: &str, key: &str, value: &str) -> Result<(), McpError> {
        let server = self.servers.get_mut(name)
            .ok_or_else(|| McpError::ServerNotFound(name.to_string()))?;
        server.headers.insert(key.to_string(), value.to_string());
        self.save()
    }

    /// Set environment variable for a server
    pub fn set_env(&mut self, name: &str, key: &str, value: &str) -> Result<(), McpError> {
        let server = self.servers.get_mut(name)
            .ok_or_else(|| McpError::ServerNotFound(name.to_string()))?;
        server.env.insert(key.to_string(), value.to_string());
        self.save()
    }

    /// Remove environment variable from a server
    pub fn remove_env(&mut self, name: &str, key: &str) -> Result<(), McpError> {
        let server = self.servers.get_mut(name)
            .ok_or_else(|| McpError::ServerNotFound(name.to_string()))?;
        server.env.remove(key);
        self.save()
    }

    /// Remove a server from config
    pub fn remove_server(&mut self, name: &str) -> Result<(), McpError> {
        if self.servers.remove(name).is_none() {
            return Err(McpError::ServerNotFound(name.to_string()));
        }
        self.save()
    }

    /// List all configured servers
    pub fn list_servers(&self) -> Vec<(&str, &McpServerConfig)> {
        self.servers.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }
}

impl McpClient {
    /// Create new MCP client
    pub fn new(config: McpConfig) -> Self {
        Self {
            config,
            processes: Arc::new(Mutex::new(HashMap::new())),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Load config and create client
    pub fn load() -> Result<Self, McpError> {
        Ok(Self::new(McpConfig::load()?))
    }

    /// Get a clone of the config
    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    /// Start an MCP server
    pub fn start_server(&self, name: &str) -> Result<(), McpError> {
        let server_config = self
            .config
            .servers
            .get(name)
            .ok_or_else(|| McpError::ServerNotFound(name.to_string()))?;

        // HTTP servers don't need to be "started" - just mark as ready
        if server_config.transport == McpTransport::Http {
            return Ok(());
        }

        let mut cmd = Command::new(&server_config.command);
        cmd.args(&server_config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for (k, v) in &server_config.env {
            cmd.env(k, v);
        }

        let child = cmd.spawn()?;

        let mut processes = self.processes.lock().unwrap();
        processes.insert(
            name.to_string(),
            McpProcess {
                child,
                request_id: 0,
            },
        );

        // Initialize the server
        drop(processes);
        self.initialize(name)?;

        Ok(())
    }

    /// Restart a server (stop then start)
    pub fn restart_server(&self, name: &str) -> Result<(), McpError> {
        self.stop_server(name)?;
        std::thread::sleep(Duration::from_millis(100));
        self.start_server(name)
    }

    /// Initialize MCP server (required after starting)
    fn initialize(&self, name: &str) -> Result<(), McpError> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "sabi-tui",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        self.call(name, "initialize", Some(params))?;
        self.call(name, "notifications/initialized", None)?;
        Ok(())
    }

    /// Call a method on an MCP server with timeout
    fn call(
        &self,
        server_name: &str,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, McpError> {
        let mut processes = self.processes.lock().unwrap();
        let process = processes
            .get_mut(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;

        process.request_id += 1;
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: process.request_id,
            method: method.to_string(),
            params,
        };

        let stdin = process.child.stdin.as_mut().ok_or_else(|| {
            McpError::ServerError("stdin not available".to_string())
        })?;

        let request_json = serde_json::to_string(&request)?;
        writeln!(stdin, "{}", request_json)?;
        stdin.flush()?;

        // For notifications, don't wait for response
        if method.starts_with("notifications/") {
            return Ok(None);
        }

        let stdout = process.child.stdout.take().ok_or_else(|| {
            McpError::ServerError("stdout not available".to_string())
        })?;

        // Read with timeout using a separate thread
        let timeout = self.timeout;
        
        let handle = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line);
            (reader.into_inner(), line, result)
        });

        // Wait for thread with timeout
        let start = std::time::Instant::now();
        loop {
            if handle.is_finished() {
                break;
            }
            if start.elapsed() > timeout {
                return Err(McpError::Timeout(timeout.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        match handle.join() {
            Ok((stdout, line, Ok(_))) => {
                // Restore stdout
                process.child.stdout = Some(stdout);
                
                if line.is_empty() {
                    return Err(McpError::ServerError("Empty response".to_string()));
                }
                
                let response: JsonRpcResponse = serde_json::from_str(&line)
                    .map_err(|e| McpError::ServerError(format!("Invalid JSON: {}", e)))?;
                    
                if let Some(err) = response.error {
                    return Err(McpError::ServerError(err.message));
                }
                Ok(response.result)
            }
            Ok((_, _, Err(e))) => Err(McpError::Io(e)),
            Err(_) => Err(McpError::ServerError("Thread panicked".to_string())),
        }
    }

    /// Call a method with auto-restart on failure
    fn call_with_retry(
        &self,
        server_name: &str,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, McpError> {
        let server_config = self.config.servers.get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;
        
        // Use HTTP transport if configured
        if server_config.transport == McpTransport::Http {
            return self.call_http(server_config, method, params);
        }
        
        match self.call(server_name, method, params.clone()) {
            Ok(result) => Ok(result),
            Err(e) => {
                // Try to restart and retry once
                if self.restart_server(server_name).is_err() {
                    return Err(e);
                }
                self.call(server_name, method, params)
            }
        }
    }

    /// Call MCP server via HTTP transport (blocking)
    fn call_http(
        &self,
        config: &McpServerConfig,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, McpError> {
        let url = config.url.as_ref()
            .ok_or_else(|| McpError::ServerError("No URL configured".to_string()))?;
        
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: method.to_string(),
            params,
        };
        
        let client = reqwest::blocking::Client::new();
        let mut req = client.post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .timeout(self.timeout);
        
        for (k, v) in &config.headers {
            req = req.header(k, v);
        }
        
        let resp = req.json(&request).send()
            .map_err(|e| McpError::ServerError(format!("HTTP error: {}", e)))?;
        
        if !resp.status().is_success() {
            return Err(McpError::ServerError(format!("HTTP {}", resp.status())));
        }
        
        let response: JsonRpcResponse = resp.json()
            .map_err(|e| McpError::ServerError(format!("Invalid JSON: {}", e)))?;
        
        if let Some(err) = response.error {
            return Err(McpError::ServerError(err.message));
        }
        
        Ok(response.result)
    }

    /// List available tools from an MCP server
    pub fn list_tools(&self, server_name: &str) -> Result<Vec<McpTool>, McpError> {
        let result = self.call_with_retry(server_name, "tools/list", None)?;

        if let Some(value) = result {
            let tools: Vec<McpTool> = serde_json::from_value(
                value.get("tools").cloned().unwrap_or(serde_json::json!([]))
            )?;
            Ok(tools)
        } else {
            Ok(vec![])
        }
    }

    /// List tools from all running servers (stdio + http)
    pub fn list_all_tools(&self) -> Result<HashMap<String, Vec<McpTool>>, McpError> {
        let mut all_tools = HashMap::new();
        
        // Stdio servers (from processes)
        let processes = self.processes.lock().unwrap();
        let stdio_names: Vec<String> = processes.keys().cloned().collect();
        drop(processes);
        
        for name in stdio_names {
            if let Ok(tools) = self.list_tools(&name) {
                all_tools.insert(name, tools);
            }
        }
        
        // HTTP servers (from config)
        for (name, config) in &self.config.servers {
            if config.transport == McpTransport::Http
                && let Ok(tools) = self.list_tools(name)
            {
                all_tools.insert(name.clone(), tools);
            }
        }
        
        Ok(all_tools)
    }

    /// Call a tool on an MCP server (with auto-retry)
    pub fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments
        });

        let result = self.call_with_retry(server_name, "tools/call", Some(params))?;
        Ok(result.unwrap_or(serde_json::json!({})))
    }

    /// Start all configured servers
    pub fn start_all(&self) -> Vec<(String, Result<(), McpError>)> {
        self.config
            .servers
            .keys()
            .map(|name| (name.clone(), self.start_server(name)))
            .collect()
    }

    /// Stop a server
    pub fn stop_server(&self, name: &str) -> Result<(), McpError> {
        let mut processes = self.processes.lock().unwrap();
        if let Some(mut process) = processes.remove(name) {
            let _ = process.child.kill();
        }
        Ok(())
    }

    /// Stop all servers
    pub fn stop_all(&self) {
        let mut processes = self.processes.lock().unwrap();
        for (_, mut process) in processes.drain() {
            let _ = process.child.kill();
        }
    }

    /// Get list of configured server names
    pub fn server_names(&self) -> Vec<String> {
        self.config.servers.keys().cloned().collect()
    }

    /// Check if a server is running
    pub fn is_running(&self, name: &str) -> bool {
        self.processes.lock().unwrap().contains_key(name)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// CLI commands for MCP management
pub fn handle_mcp_command(args: &[String]) -> Result<(), McpError> {
    if args.is_empty() {
        print_mcp_help();
        return Ok(());
    }

    match args[0].as_str() {
        "add" => {
            // Parse --transport and --header options
            let mut transport = "stdio";
            let mut headers: HashMap<String, String> = HashMap::new();
            let mut positional: Vec<&str> = vec![];
            let mut i = 1;
            
            while i < args.len() {
                match args[i].as_str() {
                    "--transport" | "-t" => {
                        i += 1;
                        if i < args.len() {
                            transport = args[i].as_str();
                        }
                    }
                    "--header" | "-H" => {
                        i += 1;
                        if i < args.len()
                            && let Some((k, v)) = args[i].split_once(':')
                        {
                            headers.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                    _ => positional.push(&args[i]),
                }
                i += 1;
            }
            
            if positional.len() < 2 {
                eprintln!("Usage: sabi mcp add [--transport stdio|http] [--header KEY:VALUE] <name> <command|url> [args...]");
                eprintln!("Examples:");
                eprintln!("  sabi mcp add filesystem npx -y @modelcontextprotocol/server-filesystem /home");
                eprintln!("  sabi mcp add -t http -H \"API-KEY: xxx\" context7 https://mcp.context7.com/mcp");
                std::process::exit(1);
            }
            
            let name = positional[0];
            let mut config = McpConfig::load()?;
            
            if transport == "http" {
                let url = positional[1];
                config.add_http_server(name, url, headers)?;
                println!("✓ Added HTTP MCP server: {} → {}", name, url);
            } else {
                let command = positional[1];
                let cmd_args: Vec<String> = positional[2..].iter().map(|s| s.to_string()).collect();
                config.add_server(name, command, cmd_args)?;
                println!("✓ Added MCP server: {}", name);
            }
        }
        "remove" | "rm" => {
            if args.len() < 2 {
                eprintln!("Usage: sabi mcp remove <name>");
                std::process::exit(1);
            }
            let name = &args[1];
            let mut config = McpConfig::load()?;
            config.remove_server(name)?;
            println!("✓ Removed MCP server: {}", name);
        }
        "env" => {
            if args.len() < 3 {
                eprintln!("Usage: sabi mcp env <name> <KEY=VALUE | -d KEY>");
                eprintln!("Example: sabi mcp env brave BRAVE_API_KEY=xxx");
                eprintln!("         sabi mcp env brave -d BRAVE_API_KEY");
                std::process::exit(1);
            }
            let name = &args[1];
            let mut config = McpConfig::load()?;
            
            if args[2] == "-d" || args[2] == "--delete" {
                // Delete env var
                if args.len() < 4 {
                    eprintln!("Usage: sabi mcp env <name> -d <KEY>");
                    std::process::exit(1);
                }
                let key = &args[3];
                config.remove_env(name, key)?;
                println!("✓ Removed env {} from {}", key, name);
            } else {
                // Set env var (KEY=VALUE)
                let kv = &args[2];
                if let Some((key, value)) = kv.split_once('=') {
                    config.set_env(name, key, value)?;
                    println!("✓ Set {}={} for {}", key, value, name);
                } else {
                    eprintln!("Invalid format. Use KEY=VALUE");
                    std::process::exit(1);
                }
            }
        }
        "list" | "ls" => {
            let config = McpConfig::load()?;
            if config.servers.is_empty() {
                println!("No MCP servers configured.");
                println!("Add one with: sabi mcp add <name> <command> [args...]");
            } else {
                println!("MCP Servers:");
                for (name, server) in &config.servers {
                    if server.transport == McpTransport::Http {
                        println!("  {} [http] → {}", name, server.url.as_deref().unwrap_or(""));
                        for (k, v) in &server.headers {
                            println!("      {}: {}", k, v);
                        }
                    } else {
                        let args_str = server.args.join(" ");
                        println!("  {} [stdio] → {} {}", name, server.command, args_str);
                        for (k, v) in &server.env {
                            println!("      {}={}", k, v);
                        }
                    }
                }
            }
        }
        "help" | "-h" | "--help" => {
            print_mcp_help();
        }
        _ => {
            eprintln!("Unknown MCP command: {}", args[0]);
            print_mcp_help();
            std::process::exit(1);
        }
    }
    Ok(())
}

fn print_mcp_help() {
    println!("MCP Server Management");
    println!();
    println!("Usage: sabi mcp <command> [args...]");
    println!();
    println!("Commands:");
    println!("  add [options] <name> <cmd|url> [args]  Add MCP server");
    println!("  remove <name>                          Remove MCP server");
    println!("  env <name> KEY=VALUE                   Set environment variable");
    println!("  env <name> -d KEY                      Remove environment variable");
    println!("  list                                   List configured servers");
    println!();
    println!("Options for 'add':");
    println!("  -t, --transport <stdio|http>  Transport type (default: stdio)");
    println!("  -H, --header <KEY:VALUE>      HTTP header (can be repeated)");
    println!();
    println!("Examples:");
    println!("  sabi mcp add filesystem npx -y @modelcontextprotocol/server-filesystem /home");
    println!("  sabi mcp add -t http -H \"API-KEY: xxx\" context7 https://mcp.context7.com/mcp");
    println!("  sabi mcp env brave BRAVE_API_KEY=your-api-key");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parse() {
        let toml = r#"
[servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home"]

[servers.git]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-git"]
env = { GIT_DIR = "/repo" }
"#;
        let config: McpConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert!(config.servers.contains_key("filesystem"));
        assert!(config.servers.contains_key("git"));
    }

    #[test]
    fn test_empty_config() {
        let config = McpConfig::default();
        assert!(!config.has_servers());
    }
}
