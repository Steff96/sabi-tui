# Sabi-TUI

A terminal-based AI agent implementing the ReAct (Reasoning + Acting) pattern for system administration. Describe tasks in natural language, review AI-generated shell commands, and get analysis of results.

![Demo](demo.gif)

## Features

- 🤖 **Multi-provider AI** - Gemini, OpenAI, Ollama, Groq, Together AI
- 💻 **Terminal access** - Execute commands with safety checks
- 🐍 **Python executor** - Run Python code for calculations (auto-detected)
- 🖼️ **Image analysis** - Paste images from clipboard or file for AI analysis
- 🔌 **MCP Support** - Extend with Model Context Protocol servers
- 🔒 **Safe mode** - Preview commands without execution
- 💾 **Multi-session** - Save and switch between conversation sessions
- 🛡️ **2-step confirmation** - Dangerous commands require explicit confirmation
- 🚫 **Interactive command blocking** - Prevents hanging on vim, ssh, etc.

## Installation

### Quick Install (Recommended)

```bash
curl -sSL https://raw.githubusercontent.com/n4ar/sabi-tui/main/setup.sh | bash
```

### Manual Download

Download from [Releases](https://github.com/n4ar/sabi-tui/releases):

| Platform | Binary |
|----------|--------|
| macOS (Apple Silicon) | `sabi-macos-aarch64` |
| macOS (Intel) | `sabi-macos-x86_64` |
| Linux (x64) | `sabi-linux-x86_64` |
| Linux (ARM64) | `sabi-linux-aarch64` |

### Build from Source

```bash
git clone https://github.com/n4ar/sabi-tui.git
cd sabi-tui
cargo build --release
cp target/release/sabi ~/.local/bin/
```

## Quick Start

Run `sabi` and follow the onboarding wizard:

```
🚀 Welcome to Sabi-TUI!

Select provider:
  1) Gemini (Google AI)
  2) OpenAI
  3) OpenAI-compatible (Ollama, Groq, Together, etc.)

Choice [1]: 
```

## Configuration

All config stored in `~/.sabi/`:

```toml
# ~/.sabi/config.toml

# Provider: "gemini" or "openai"
provider = "gemini"
api_key = "your-api-key"
model = "gemini-2.5-flash"

# For OpenAI-compatible APIs (Ollama, Groq, etc.)
# provider = "openai"
# base_url = "http://localhost:11434/v1"
# model = "llama3.2"
```

### Provider Examples

```toml
# Gemini
provider = "gemini"
api_key = "your-gemini-key"
model = "gemini-2.5-flash"

# OpenAI
provider = "openai"
api_key = "sk-xxx"
model = "gpt-4o"

# Ollama (local)
provider = "openai"
base_url = "http://localhost:11434/v1"
model = "llama3.2"

# Groq
provider = "openai"
base_url = "https://api.groq.com/openai/v1"
api_key = "gsk_xxx"
model = "llama-3.3-70b-versatile"
```

## Usage

```bash
sabi                    # Interactive TUI mode
sabi -q "prompt"        # Quick query (text response)
sabi -x "prompt"        # Execute mode (with confirmation)
sabi --safe             # Safe mode (preview only)
sabi mcp <command>      # Manage MCP servers
sabi --version          # Show version
sabi --help             # Show help
```

### Quick CLI Mode

Get instant answers without entering the TUI:

```bash
# Quick query - get text response
sabi -q "what is my IP address"

# Execute mode - run command with confirmation dialog
sabi -x "list large files in current directory"
```

Execute mode (`-x`) shows:
1. **Confirmation dialog** - Review command before execution
2. **Result dialog** - Output with AI summary

### Slash Commands

| Command | Description |
|---------|-------------|
| `/model [name]` | List or switch AI model |
| `/new` | Start new session |
| `/sessions` | List all sessions |
| `/switch <id>` | Switch to session |
| `/delete <id>` | Delete session |
| `/image <path>` | Analyze image file |
| `/usage` | Show token usage stats |
| `/export [file]` | Export chat to markdown |
| `/clear` | Clear chat history |
| `/help` | Show help |
| `/quit` | Exit |

Press `Tab` to autocomplete commands.

### Shell Escape

Use `!` prefix to run shell commands directly without AI:

```bash
!ls -la          # List files
!pwd             # Print working directory
!cat file.txt    # View file contents
```

### Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Submit / Execute |
| `Esc` | Cancel / Quit |
| `Tab` | Autocomplete |
| `Ctrl+O` | Paste image from clipboard |
| `↑`/`↓` | Scroll history |
| `Ctrl+C` | Force quit |

## MCP (Model Context Protocol)

Extend Sabi with external tools via MCP servers (stdio or HTTP).

### Managing MCP Servers

```bash
# Add stdio server
sabi mcp add <name> <command> [args...]

# Add HTTP server
sabi mcp add -t http -H "API-KEY: xxx" <name> <url>

# Set environment variable (stdio)
sabi mcp env <name> KEY=VALUE

# List configured servers
sabi mcp list

# Remove a server
sabi mcp remove <name>
```

### Examples

```bash
# Filesystem server (stdio)
sabi mcp add filesystem npx -y @modelcontextprotocol/server-filesystem /home

# Git server (stdio)
sabi mcp add git npx -y @modelcontextprotocol/server-git

# Brave Search with API key (stdio)
sabi mcp add brave npx -y @anthropic/mcp-server-brave-search
sabi mcp env brave BRAVE_API_KEY=your-api-key

# Context7 (HTTP)
sabi mcp add -t http -H "CONTEXT7_API_KEY: your-key" context7 https://mcp.context7.com/mcp

# HTTP server with multiple headers
sabi mcp add -t http -H "Authorization: Bearer xxx" -H "X-Custom: value" myserver https://example.com/mcp
```

### MCP Configuration File

Servers are stored in `~/.sabi/mcp.toml`. See [examples/mcp.toml.example](examples/mcp.toml.example) for a complete example.

## Safety Features

### 🛡️ 2-Step Confirmation for Dangerous Commands

Commands targeting sensitive paths (`~`, `/Users`, `/etc`) or using destructive patterns (`rm -rf`) require:

1. **First Enter** - Warning displayed
2. **Second Enter** - Must type "I understand the risks"

```
⚠️ DANGEROUS COMMAND DETECTED!
This command could cause irreversible damage.
Press Enter again to proceed to final confirmation.

🛑 FINAL CONFIRMATION REQUIRED
Type exactly: I understand the risks
```

### ⛔ Unknown Tool Blocking

AI cannot create arbitrary tools. Only allowed:
- `run_cmd` - Shell commands
- `run_python` - Python code
- `read_file` / `write_file` - File operations
- `search` - File search
- `mcp` - MCP server tools

### 🚫 Dangerous Path Detection

Operations on these paths trigger safety checks:
- Home directories: `~`, `/Users`, `/home`, `/root`
- System directories: `/etc`, `/var`, `/usr`, `/bin`, `/sbin`
- macOS system: `/System`, `/Library`, `/Applications`

## Available Tools

| Tool | Description |
|------|-------------|
| `run_cmd` | Execute shell command |
| `run_python` | Execute Python code |
| `read_file` | Read file contents |
| `write_file` | Write to file |
| `search` | Search for files |
| `mcp` | Call MCP server tools |

## Troubleshooting

### "API key not found"
Run `sabi` to start onboarding, or edit `~/.sabi/config.toml`

### Python not detected
Install Python 3: `brew install python3` (macOS) or `apt install python3` (Linux)

### Model not working
Use `/model` to list available models and switch

### MCP server not starting
- Check if the command exists: `npx -y @modelcontextprotocol/server-filesystem --help`
- Verify environment variables: `sabi mcp list`
- Check server logs in stderr

## Uninstall

```bash
# Using uninstall script
curl -sSL https://raw.githubusercontent.com/n4ar/sabi-tui/main/uninstall.sh | bash

# Or manually
rm ~/.local/bin/sabi
rm -rf ~/.sabi
```

## License

MIT
