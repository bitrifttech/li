#!/bin/bash

# Installation script for li CLI tool
set -e

echo "🚀 Installing li CLI tool..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "🦀 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Install from source using cargo
echo "📦 Installing li from source..."
cargo install --path .

echo "✅ li installed successfully!"
echo ""
echo "🎯 Next steps:"
echo "1. Configure your Cerebras API key:"
echo "   li config --api-key YOUR_CEREBRAS_API_KEY"
echo ""
echo "2. Try it out:"
echo "   li 'list files in current directory'"
echo ""
echo "3. For help:"
echo "   li --help"
