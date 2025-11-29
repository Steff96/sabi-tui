# agent-rs

A terminal-based AI agent implementing the ReAct (Reasoning + Acting) pattern for system administration. Describe tasks in natural language, review AI-generated shell commands, and get analysis of results.

## Features

- Natural language to shell command translation
- Command review and editing before execution
- Output capture and AI-powered analysis
- Dangerous command detection with visual warnings
- Conversation history for context-aware interactions

## Requirements

- Rust 2024 edition
- Gemini API key (get one at https://aistudio.google.com/apikey)

## Installation

```bash
git clone <repository-url>
cd agent-rs
cargo build --release
```

## Configuration

### Option 1: Environment Variable (Recommended for quick start)

```bash
export GEMINI_API_KEY="your-gemini-api-key"
```

### Option 2: Config File

Create `~/.config/agent-rs/config.toml`:

```toml
api_key = "your-gemini-api-key"
model = "gemini-2.5-flash"           # optional, default: gemini-2.5-flash
max_history_messages = 20            # optional, conversation context limit
max_output_bytes = 10000             # optional, command output truncation
max_output_lines = 100               # optional, command output line limit
dangerous_patterns = ["rm -rf", "mkfs", "dd if=", "> /dev/"]  # optional
```

## Usage

### Starting the Application

```bash
# If built with --release
./target/release/agent-rs

# Or run directly with cargo
cargo run --release
```

### Basic Workflow

1. **Type your query** in natural language at the input prompt
   ```
   > list all files larger than 100MB in home directory
   ```

2. **Review the proposed command** - AI will suggest a shell command
   ```
   ┌─ Command (Enter to execute, Esc to cancel) ─┐
   │ find ~ -type f -size +100M -exec ls -lh {} \;│
   └──────────────────────────────────────────────┘
   ```

3. **Edit if needed** - You can modify the command before execution

4. **Execute or Cancel**
   - Press `Enter` to execute the command
   - Press `Esc` to cancel and return to input

5. **View results** - The output is captured and analyzed by AI

6. **Continue the conversation** - Ask follow-up questions with context preserved

### Example Session

```
You: show disk usage of current directory
AI: I'll check the disk usage for you.

┌─ Command ─────────────────────────┐
│ du -sh ./*                        │
└───────────────────────────────────┘

[Press Enter to execute]

Output:
4.0K    ./Cargo.toml
156M    ./target
12K     ./src

AI: The current directory uses about 156MB total, mostly from the 
    target/ build directory. The source code in src/ is only 12KB.

You: clean up the build artifacts
...
```

### Dangerous Command Warning

Commands matching dangerous patterns (like `rm -rf`) will show a red warning:

```
┌─ ⚠ DANGEROUS COMMAND - Review Carefully! ─┐
│ rm -rf ./target                            │
└────────────────────────────────────────────┘
```

### Keybindings

| State | Key | Action |
|-------|-----|--------|
| Input | `Enter` | Submit query to AI |
| Input | `Esc` | Quit application |
| Input | `↑`/`↓` | Scroll chat history |
| Input | Any key | Type in input field |
| Review | `Enter` | Execute the command |
| Review | `Esc` | Cancel and return to input |
| Review | Any key | Edit the command |
| Thinking/Executing | `Esc` | Emergency quit |
| Any | `Ctrl+C` | Force quit |

### UI Layout

```
┌─────────────────────────────────────┐
│           Chat History              │  ← Conversation with AI
│  You: list files                    │
│  AI: Here are the files...          │
├─────────────────────────────────────┤
│  Command / Output / Spinner         │  ← Context-dependent middle pane
├─────────────────────────────────────┤
│ [INPUT] Enter: Submit | Esc: Quit   │  ← Status bar with keybindings
└─────────────────────────────────────┘
```

## Architecture

```
Input → Thinking → ReviewAction → Executing → Finalizing → Input
          ↓              ↓
        (text)       (cancel)
          ↓              ↓
        Input ←──────────┘
```

The app follows a state machine pattern with async event handling for responsive UI during API calls and command execution.

### States

| State | Description |
|-------|-------------|
| Input | Waiting for user query |
| Thinking | AI is processing the request |
| ReviewAction | Displaying command for user review |
| Executing | Running the shell command |
| Finalizing | AI is analyzing command output |

## Troubleshooting

### "API key not found"
Set the `GEMINI_API_KEY` environment variable or create a config file.

### "Terminal too small"
Resize your terminal to at least 40x10 characters.

### Command not executing
Make sure you're pressing `Enter` in the Review state, not `Esc`.

## License

MIT
