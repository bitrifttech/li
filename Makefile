.PHONY: install install-dev test clean release brew-test

# Install from source
install:
	./install.sh

# Install for development
install-dev:
	cargo install --path .

# Run tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean

# Build release binary
release:
	cargo build --release

# Test Homebrew formula locally
brew-test:
	brew install --formula ./li.rb --verbose

# Create a new release
release-patch: test release
	@echo "Creating patch release..."
	@cargo bump patch
	@git add Cargo.toml
	@git commit -m "Bump version to $$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')"
	@git tag "v$$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')"
	@echo "Release created! Run 'git push origin v$$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')' to publish"

# Update Homebrew formula
update-brew:
	@echo "Updating SHA256 in formula..."
	@curl -sL https://github.com/bitrifttech/li/archive/refs/tags/v$$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version').tar.gz | sha256sum | cut -d' ' -f1 > /tmp/sha256.txt
	@sed -i.bak "s/sha256 \".*\"/sha256 \"$$(cat /tmp/sha256.txt)\"/" li.rb
	@rm li.rb.bak /tmp/sha256.txt
	@echo "Formula updated. Commit and push to update the tap."

# Help
help:
	@echo "Available targets:"
	@echo "  install      - Install li from source"
	@echo "  install-dev  - Install for development"
	@echo "  test         - Run tests"
	@echo "  clean        - Clean build artifacts"
	@echo "  release      - Build release binary"
	@echo "  brew-test    - Test Homebrew formula"
	@echo "  release-patch - Create patch release"
	@echo "  update-brew  - Update Homebrew formula SHA256"
	@echo "  help         - Show this help"
