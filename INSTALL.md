# Installation

## Option 1: Install from Source (Recommended)

```bash
# Clone the repository
git clone https://github.com/bitrifttech/li.git
cd li

# Run the installation script
./install.sh
```

## Option 2: Manual Installation

### Prerequisites
- [Homebrew](https://brew.sh/) installed
- [Rust](https://rustup.rs/) installed (or install via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

### Installation Steps

```bash
# Install from the formula
brew install --formula ./li.rb

# Or install directly with cargo
cargo install --path .
```

## Option 3: Install from Homebrew Tap

```bash
brew tap bitrifttech/homebrew-li
brew install li
```

## Post-Installation

1. **Configure your OpenRouter API key:**
   ```bash
   # Get your API key from https://openrouter.ai/
   li config --api-key YOUR_OPENROUTER_API_KEY
   ```

2. **Verify installation:**
   ```bash
   li --help
   li 'list files in current directory'
   ```

## Configuration

The CLI stores configuration in `~/.li/config.json`. You can edit this file directly or use the config commands:

```bash
# View current config
li config

# Set API key
li config --api-key your-key-here

# Set custom models
li config --classifier-model nvidia/nemotron-nano-12b-v2-vl:free
li config --planner-model minimax/minimax-m2:free
```

## Troubleshooting

### "Command not found" error
Make sure `~/.cargo/bin` is in your PATH:
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### Build issues
Ensure you have the latest Rust:
```bash
rustup update
```

### API key issues
Verify your OpenRouter API key is valid and has sufficient credits.

## Uninstall

```bash
# If installed via Homebrew
brew uninstall li

# If installed via cargo
cargo uninstall li

# Remove configuration
rm -rf ~/.li
```
