# Changelog

## [0.1.7] - 2025-12-03

### Added
- 🔌 **MCP Support** - Extend Sabi with Model Context Protocol servers
- 🛠️ **MCP CLI** - Manage servers via `sabi mcp add/remove/list/env`
- 🔑 **Environment variables** - `sabi mcp env <name> KEY=VALUE` to set API keys
- 🔄 **Auto-restart** - MCP servers automatically restart on failure
- ⏱️ **30s timeout** - MCP calls timeout after 30 seconds to prevent hanging
- 📄 **Auto-create mcp.toml** - Config file created during onboarding
- 🐚 **Shell escape** - Use `!command` to run shell commands directly without AI

### Changed
- MCP tool execution is now async (non-blocking UI)
- Added `mcp` to allowed tools list

## [0.1.6] - 2025-11-30

### Added
- ⚡ **Quick CLI mode** - `sabi -q "prompt"` for text response, `sabi -x "prompt"` for execution
- 🔔 **Auto-update check** - Notifies when new version available on startup
- 🎯 **Execute confirmation dialog** - TUI dialog showing AI explanation before running commands
- 📊 **Result dialog with AI summary** - Shows output and AI-generated summary after execution

### Changed
- `-x` mode now shows full TUI confirmation and result dialogs

## [0.1.5] - 2025-11-30

### Added
- 🤖 **Multi-provider support** - OpenAI, Ollama, Groq, Together AI via OpenAI-compatible API
- 🚀 **Onboarding wizard** - First-run setup to choose provider and configure API key
- 🔄 **Model switching** - `/model` command to list and switch models
- ⌨️ **Tab autocomplete** - Press Tab to autocomplete slash commands
- 🛡️ **Enhanced security** - 2-step confirmation for dangerous commands
- ⛔ **Unknown tool blocking** - Blocks AI-generated tools not in allowed list
- 🚨 **Dangerous path detection** - Blocks operations on `~`, `/Users`, `/etc`, etc.

### Changed
- Config path unified to `~/.sabi/` (config.toml + sessions/)
- Dangerous commands now require typing "I understand the risks" to execute

### Security
- LLM cannot bypass safety checks by creating fake tools
- Destructive operations require explicit user confirmation
- Path-based restrictions prevent accidental system damage

## [0.1.4] - 2025-11-30

### Added
- 🖼️ **Image analysis** - `/image <path> [prompt]` command and `Ctrl+O` to paste from clipboard
- 📊 **Usage stats** - `/usage` command shows session token estimates and context window usage
- 📤 **Export chat** - `/export [filename.md]` exports conversation history to markdown

## [0.1.3] - 2025-11-30

### Added
- 🐍 **Python executor** - Run Python code with `run_python` tool (auto-detected at startup)
- 🔒 **Safe mode** - Preview commands without execution (`sabi --safe`)
- 💾 **Multi-session support** - `/new`, `/sessions`, `/switch <id>`, `/delete <id>`
- 🚫 **Interactive command blocking** - Detects and blocks vim, ssh, htop, etc. with suggestions
- ⏹️ **Cancel running commands** - Press `Esc` during execution to abort
- 🖥️ **System context** - AI knows current time, user, shell, OS, and working directory
- 📦 **Pre-built binaries** - Download from GitHub Releases (macOS, Linux)
- 🐟 **Fish shell support** - setup.sh now supports fish

### Changed
- Renamed project from `agent-rs` to `Sabi-TUI`
- Binary name changed to `sabi`
- Config path: `~/.sabi/`
- Environment variable: `SABI_API_KEY`
- Middle pane now auto-sizes based on content
- Switched to `rustls` for cross-compilation support

### Fixed
- Commands no longer hang on interactive programs
- Session auto-save on exit

## [0.1.0] - 2025-11-29

### Added
- Initial release
- Gemini AI integration
- ReAct pattern implementation
- Shell command execution
- File read/write tools
- Dangerous command detection
- TUI interface with ratatui
