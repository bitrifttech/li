# li - AI-Powered CLI Assistant

<!-- li Icon -->
<p align="center">
  <img src="resource/li_logo.png" alt="li Logo" width="240" height="240">
</p>

🚀 **li** is a lightweight terminal assistant that converts natural language to shell commands. Just type plain English like "make a new git repo" and li will generate a safe, minimal command plan for you to review and execute.

## ✨ Features

- 🧠 **Natural Language to Commands**: Type plain English, get shell commands
- 🛡️ **Safe Execution**: Every plan is previewed before execution
- 🎯 **Smart Classification**: Automatically distinguishes between shell commands and natural language tasks
- 💬 **Direct AI Chat**: Use `--chat` flag for conversational AI assistance
- 🧠 **AI Intelligence Mode**: Use `-i` flag to explain command outputs in human-friendly terms
- 🌐 **Provider Choice**: Switch between OpenRouter and Cerebras with `li --provider`
- 🔧 **Interactive Setup**: Easy first-time configuration with `li --setup`
- 🎨 **Visual Separators**: Clear distinction between li output and command output
- 📋 **Model Selection**: Browse OpenRouter's free models when using that provider
- 🪝 **Shell Hook Integration**: Optional zsh hook for seamless terminal experience

## 🚀 Quick Start

### Installation

#### Option 1: Install from Source (Recommended)
```bash
git clone https://github.com/bitrifttech/li.git
cd li
./install.sh
```

#### Option 2: Install via Homebrew Tap
```bash
brew tap bitrifttech/homebrew-li
brew install li
```

#### Option 3: Manual Cargo Install
```bash
cargo install --git https://github.com/bitrifttech/li.git
```

### First-Time Setup

1. **Run interactive setup**:
   ```bash
   li --setup
   ```
   
   This will guide you through:
   - Choosing your AI provider (OpenRouter or Cerebras)
   - Supplying the provider API key
   - Selecting classifier and planner models (OpenRouter only)
   - Configuring timeout and token limits

2. **Add your provider API key**:
   - **OpenRouter**:
     - Visit [https://openrouter.ai/](https://openrouter.ai/)
     - Sign up for a free account
     - Copy your API key (starts with `sk-or-v1-`)
   - **Cerebras**:
     - Use your Cerebras Inference API key (set via the Cerebras account dashboard)
     - Export it as `CEREBRAS_API_KEY` or provide it during setup

3. **Try it out**:
   ```bash
   li 'list all files in current directory'
   li 'create a new git repository'
   li 'show system disk usage'
   ```

## 📖 Usage

### Basic Usage

```bash
# Plan and execute commands
li 'list files in current directory'
li 'make a new git repo and connect to GitHub'
li 'find the 10 largest files in this folder'

# Direct AI conversation
li --chat 'what is the capital of France?'
li --chat 'explain quantum computing simply'

# AI Intelligence Mode - explain command outputs or answer questions
li -i 'df -h'                                    # Explain disk usage output
li --intelligence 'ps aux'                       # Understand running processes
li -i 'mount'                                    # Learn about mounted filesystems
li -i --question 'Which disk has most space?' "df -h"  # Ask a specific question
li -i 'ls -la'                                   # Understand file permissions

# Interactive model selection
li --model
li --model list

# Provider selection
li --provider
li --provider list

# Manual configuration
li config --api-key YOUR_OPENROUTER_API_KEY
li config --classifier-model nvidia/nemotron-nano-12b-v2-vl:free
li config --planner-model minimax/minimax-m2:free
```

### Command Options

```bash
li --help                    # Show all options
li --setup                   # Interactive first-time setup
li --chat "message"          # Direct AI conversation
li -i "command"              # Explain command output with AI
li --intelligence "command"  # Long form of -i flag
li --model                   # Interactive model selection
li --model list              # Show available models
li --classify "command"      # Classify input only (for shell hooks)
li config                    # View current configuration
```

### Examples

#### File Operations
```bash
li 'list all files including hidden ones'
li 'create a backup of this directory'
li 'find all Python files in current folder'
li 'remove all .log files older than 30 days'
```

#### Git Operations
```bash
li 'initialize a new git repository'
li 'add all files and make initial commit'
li 'create a new branch called feature-x'
li 'merge develop branch into main'
```

#### System Information
```bash
li 'show system disk usage'
li 'list all mounted drives'
li 'check system memory usage'
li 'show running processes'
```

#### Development Tasks
```bash
li 'install npm dependencies'
li 'run the development server'
li 'build the project for production'
li 'run all tests'
```

## 🧠 AI Intelligence Mode

The **intelligence mode** (`-i` or `--intelligence`) helps you understand command outputs by running a command and then using AI to explain what the output means in human-friendly terms.

### How It Works

1. **Execute Command**: li runs your specified shell command
2. **Capture Output**: Both stdout and stderr are collected
3. **AI Explanation**: The output is sent to the AI model for analysis
4. **Human-Friendly Breakdown**: Get explanations, insights, and warnings

### Examples

#### System Information
```bash
# Understand disk usage
li -i "df -h"
li -i --question "Which disk has most free space?" "df -h"

# Learn about mounted filesystems
li --intelligence "mount"

# Analyze running processes
li -i "ps aux | head -20"
```

#### File Operations
```bash
# Understand file permissions
li -i "ls -la /etc"

# Analyze directory structure
li --intelligence "tree -L 2"

# Check file sizes
li -i "du -sh * | sort -hr | head -10"
```

#### Network & System
```bash
# Network connections
li -i "netstat -an | grep LISTEN"

# System resources
li --intelligence "top -l 1 | head -15"
li --intelligence --question "Where is CPU usage spiking?" "top -l 1 | head -15"

# Memory usage
li -i "vm_stat"
```

### What You Get

Each intelligence explanation provides:
- **Simple Meaning**: What the output means in plain English
- **Key Insights**: Important information and patterns
- **Warnings**: Things to pay attention to or avoid
- **Practical Understanding**: What you should do with this information

### Use Cases

- **Learning**: Understand unfamiliar commands
- **Troubleshooting**: Get insights into system issues
- **Security**: Analyze what's running on your system
- **Optimization**: Identify resource usage patterns

## ⚙️ Configuration

### Configuration File

li stores configuration in `~/.li/config` (JSON format):

```json
{
  "openrouter_api_key": "sk-or-v1-your-api-key",
  "timeout_secs": 30,
  "max_tokens": 2048,
  "classifier_model": "nvidia/nemotron-nano-12b-v2-vl:free",
  "planner_model": "minimax/minimax-m2:free"
}
```

### Environment Variables

You can override configuration with environment variables:

```bash
export OPENROUTER_API_KEY="sk-or-v1-your-api-key"
export CEREBRAS_API_KEY="cb-your-api-key"
export LI_PROVIDER="openrouter"          # or 'cerebras'
export LI_LLM_BASE_URL="https://openrouter.ai/api/v1"
export LI_TIMEOUT_SECS="60"
export LI_MAX_TOKENS="4096"
export LI_CLASSIFIER_MODEL="nvidia/nemotron-nano-12b-v2-vl:free"
export LI_PLANNER_MODEL="minimax/minimax-m2:free"
```

### Configuration Commands

```bash
# Set API key
li --config --api-key sk-or-v1-your-key

# Set custom models
li --config --classifier-model nvidia/nemotron-nano-12b-v2-vl:free
li --config --planner-model minimax/minimax-m2:free

# Adjust settings
li --config --timeout 60
li --config --max-tokens 4096

# Switch providers on the fly
li --provider cerebras
```

## 🤖 AI Models

li ships with OpenRouter defaults and supports additional providers such as Cerebras.

### OpenRouter Defaults
- **Classifier**: `nvidia/nemotron-nano-12b-v2-vl:free` - Fast, accurate command classification
- **Planner**: `minimax/minimax-m2:free` - Intelligent shell command planning

### Available Free Models
```bash
li --model list    # Show all available free models
li --model         # Interactive model selection
```

### Cerebras Models
- Provide model IDs from your Cerebras workspace during setup or via `li --config`
- Use `CEREBRAS_API_KEY` and optional `LI_LLM_BASE_URL` to target custom deployments

## 🪝 Shell Integration (Optional)

For a seamless experience, install the zsh hook to automatically route natural language through li:

```bash
# Install the hook
li install

# Restart your shell or run:
source ~/.zshrc
```

After installation:
- Type `ls -la` → Executes directly (classified as terminal command)
- Type "show all files" → Routes through li (classified as natural language)

```bash
# Uninstall the hook
li uninstall
```

## 🎨 Output Examples

### Command Planning
_Example output using the OpenRouter provider_
```bash
$ li 'create a new git repository'

Provider: OpenRouter
Model: minimax/minimax-m2:free
Plan confidence: 1.00

Dry-run Commands:
  1. git status

Execute Commands:
  1. git init
  2. git add .
  3. git commit -m "Initial commit"

Notes: Created minimal git repo with initial commit.

Execute this plan? [y/N]: y

=== Executing Plan ===

[Dry-run Phase]

> Running check 1/1: git status

┌─ COMMAND OUTPUT: git status
│
│ fatal: not a git repository (or any of the parent directories)
│
└─ Command completed successfully

✓ All dry-run checks passed.

[Execute Phase]

> Executing 1/3: git init
┌─ COMMAND OUTPUT: git init
│
│ Initialized empty Git repository in /path/to/repo/.git/
│
└─ Command completed successfully

> Executing 2/3: git add .
> Executing 3/3: git commit -m "Initial commit"

✓ Plan execution completed.
```

### Direct Chat
_Example output using the OpenRouter provider_
```bash
$ li --chat "what is the capital of France?"

Provider: OpenRouter
Model: minimax/minimax-m2:free

Choice 1:
The capital of France is **Paris**. It's also famous for landmarks like the Eiffel Tower and the Louvre Museum.
Finish reason: stop
```

## 🔧 Troubleshooting

### Common Issues

#### "Command not found" error
```bash
# Add cargo to PATH (if using cargo install)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

#### API Key Issues
```bash
# Verify your API key is valid
li config

# Get a new key from https://openrouter.ai/
li config --api-key sk-or-v1-your-new-key
```

#### Network Issues
```bash
# Test connectivity
curl -I https://openrouter.ai/

# Check if behind a proxy
export HTTPS_PROXY=your-proxy-url
```

#### Build Issues
```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cargo clean
cargo build --release
```

### Debug Mode

Set `LI_LOG_DIR` to enable debug logging:
```bash
export LI_LOG_DIR="/tmp/li-logs"
li 'test command'
# Logs will be written to /tmp/li-logs/
```

## 🏗️ Development

### Building from Source

```bash
git clone https://github.com/bitrifttech/li.git
cd li

# Install dependencies
cargo build

# Run tests
cargo test

# Install locally
cargo install --path .
```

### Project Structure

```
src/
├── main.rs              # Entry point
├── cli.rs               # CLI arguments and commands
├── config.rs            # Configuration management
├── client.rs            # LLM provider client (OpenRouter, Cerebras)
├── classifier/          # Command classification logic
├── planner/             # Command planning logic
├── exec/                # Command execution
└── hook/                # Shell integration
```

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests (requires API key)
OPENROUTER_API_KEY=your-key cargo test --test integration_test
```

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 🆘 Support

- 📖 [Documentation](https://github.com/bitrifttech/li/tree/main/documentation)
- 🐛 [Issue Tracker](https://github.com/bitrifttech/li/issues)
- 💬 [Discussions](https://github.com/bitrifttech/li/discussions)

## 🎯 Roadmap

### v1.1 (Planned)
- [ ] Bash/Fish shell hooks
- [ ] Better portability shims (BSD vs GNU utilities)
- [ ] Command history and favorites
- [ ] Custom command templates

### v2.0 (Future)
- [ ] Code generation and multi-file scaffolding
- [ ] Windows support
- [ ] Local model support
- [ ] Plugin system

---

**Made with ❤️ by the bitrifttech team**

Transform your terminal experience with AI-powered natural language command generation! 🚀
