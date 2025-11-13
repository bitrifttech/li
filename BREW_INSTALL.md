# Homebrew Installation Guide

## Quick Install

### Option 1: Install from Source (Easiest)

```bash
# Clone and install
git clone https://github.com/bitrifttech/li.git
cd li
./install.sh
```

### Option 2: Install via Homebrew Tap

```bash
# Add the tap and install
brew tap bitrifttech/homebrew-li
brew install li
```

### Option 3: Manual Cargo Install

```bash
# Install directly with cargo
cargo install --git https://github.com/bitrifttech/li.git
```

## What Gets Installed

- **Binary**: `li` command-line tool
- **Config**: Stored in `~/.li/config.json`
- **Dependencies**: Rust toolchain (if not already installed)

## Post-Installation Setup

1. **Configure API Key**:
   ```bash
   # Get your key from https://openrouter.ai/
   li config --api-key YOUR_OPENROUTER_API_KEY
   ```

2. **Verify Installation**:
   ```bash
   li --help
   li 'what files are in this directory?'
   ```

## Configuration Options

Edit `~/.li/config.json` or use commands:

```bash
# View current config
li config

# Set planner model
li config --planner-model minimax/minimax-m2:free

# Adjust timeout and tokens
li config --timeout 60
li config --max-tokens 2048
```

## Troubleshooting

### Command not found
```bash
# Add cargo to PATH (if using cargo install)
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### Permission denied
```bash
# If cargo install fails due to permissions
cargo install --git https://github.com/bitrifttech/li.git --root /usr/local
```

### Update to latest version
```bash
# If installed via tap
brew upgrade li

# If installed via cargo
cargo install --git https://github.com/bitrifttech/li.git --force
```

## Uninstall

```bash
# If installed via tap
brew uninstall li
brew untap bitrifttech/homebrew-li

# If installed via cargo
cargo uninstall li

# Remove configuration
rm -rf ~/.li
```

## Development

To install from local source:

```bash
git clone https://github.com/bitrifttech/li.git
cd li
cargo install --path .
```

To create a new release:

1. Update version in `Cargo.toml`
2. Create git tag: `git tag v0.1.0`
3. Push tag: `git push origin v0.1.0`
4. Update formula in homebrew-li tap
