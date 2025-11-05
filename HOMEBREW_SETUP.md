# Homebrew Tap Setup Guide

This guide explains how to set up the Homebrew tap for distributing the `li` CLI tool.

## 📁 Files Created

The following files have been created in the root directory for Homebrew distribution:

```
homebrew-li/
├── README.md          # Tap documentation
├── li.rb              # Homebrew formula
├── setup.sh           # Tap setup script
└── update-sha.sh      # SHA256 update script
```

## 🚀 Quick Setup

### 1. Run the Setup Script

```bash
cd homebrew-li
./setup.sh
```

This will:
- Check for required tools (Homebrew, Git)
- Initialize the git repository
- Provide step-by-step instructions

### 2. Create GitHub Repository

1. Go to https://github.com/new
2. Repository name: `homebrew-li`
3. Description: `Homebrew tap for li CLI tool`
4. Make it **public**
5. Don't initialize with README (we already have one)

### 3. Push to GitHub

```bash
cd homebrew-li
git remote add origin git@github.com:bitrifttech/homebrew-li.git
git branch -M main
git push -u origin main
```

### 4. Test the Tap

```bash
# Add the tap
brew tap bitrifttech/homebrew-li https://github.com/bitrifttech/homebrew-li.git

# Install li
brew install li

# Test installation
li --help
```

## 📦 Publishing Updates

### When You Release a New Version

1. **Create a GitHub Release** in the main `li` repository
2. **Update the Formula** with the new SHA256:

```bash
cd homebrew-li
./update-sha.sh v0.1.1
```

3. **Commit and Push**:

```bash
git add li.rb
git commit -m "Update to v0.1.1"
git push
```

4. **Users Update**:

```bash
brew upgrade li
```

## 🔧 Formula Details

The `li.rb` formula includes:

- **Dependencies**: Rust toolchain (installed automatically)
- **Build Process**: Cargo install from source
- **Tests**: Basic functionality test
- **Verification**: Checks for OpenRouter API key error handling

### Key Formula Features

```ruby
class Li < Formula
  desc "CLI assistant that converts natural language to shell plans using AI"
  homepage "https://github.com/bitrifttech/li"
  url "https://github.com/bitrifttech/li/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "sha256_placeholder"  # Updated with update-sha.sh
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--bin", "li", "--path", ".", "--release"
    bin.install "target/release/li"
  end

  test do
    system "#{bin}/li", "--help"
    output = shell_output("#{bin}/li 'test' 2>&1", 1)
    assert_match(/li CLI is initialized|OpenRouter API key/, output)
  end
end
```

## 📋 Maintenance Tasks

### Regular Updates

1. **Update Dependencies**: Keep Rust version current
2. **Test on Clean Systems**: Verify installation works
3. **Monitor Issues**: Watch for Homebrew-specific problems
4. **Update Documentation**: Keep README current

### Version Release Checklist

- [ ] Update version in `Cargo.toml`
- [ ] Create git tag: `git tag v0.1.1`
- [ ] Push tag: `git push origin v0.1.1`
- [ ] Create GitHub release
- [ ] Update formula SHA256: `./update-sha.sh v0.1.1`
- [ ] Commit and push formula update
- [ ] Test installation: `brew install li`

## 🛠️ Advanced Configuration

### Custom Tap URL

If you want to use a different URL for the tap:

```bash
brew tap bitrifttech/homebrew-li https://your-custom-url/homebrew-li.git
```

### Local Testing

To test the formula locally before publishing:

```bash
# Install from local formula
brew install --formula ./li.rb

# Test installation
li --version

# Uninstall local version
brew uninstall li
```

### Formula Validation

Validate the formula before committing:

```bash
brew audit li
brew style li
```

## 📊 Analytics (Optional)

Homebrew provides analytics for tap usage:

```bash
brew analytics
brew analytics on
```

This helps track installations but is optional and user-controlled.

## 🔍 Troubleshooting

### Common Issues

**Formula fails to install:**
```bash
# Check formula syntax
brew audit li

# Install with verbose output
brew install --verbose li
```

**SHA256 mismatch:**
```bash
# Recalculate SHA256
./update-sha.sh v0.1.0

# Verify manually
curl -L https://github.com/bitrifttech/li/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
```

**Permission issues:**
```bash
# Fix Homebrew permissions
sudo chown -R $(whoami) /opt/homebrew/
```

### Getting Help

- Homebrew Documentation: https://docs.brew.sh/
- Formula Cookbook: https://docs.brew.sh/Formula-Cookbook
- Homebrew Issues: https://github.com/Homebrew/brew/issues

## 📚 Additional Resources

- **Homebrew Tap Guide**: https://docs.brew.sh/Taps
- **Formula API Reference**: https://rubydoc.brew.sh/Formula
- **Testing Homebrew Formulae**: https://docs.brew.sh/Formula-Cookbook#tests

## 🚀 Next Steps

1. **Run the setup script**: `cd homebrew-li && ./setup.sh`
2. **Create GitHub repository**: Follow the script instructions
3. **Test installation**: Verify everything works
4. **Create first release**: Tag and release v0.1.0
5. **Update formula**: Use `update-sha.sh` to set correct SHA256
6. **Announce**: Share the tap with users!

Your Homebrew tap is now ready for distribution! 🎉
