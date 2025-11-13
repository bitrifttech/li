#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR"
HOMEBREW_TAP_DIR="${SCRIPT_DIR}/../homebrew-li"

if [ ! -d "$HOMEBREW_TAP_DIR" ]; then
    echo "❌ Homebrew tap directory not found at $HOMEBREW_TAP_DIR"
    echo "   Clone https://github.com/bitrifttech/li-homebrew next to this repository."
    exit 1
fi

echo "🚀 Starting li release process..."

VERSION=${1:-"0.1.5"}

if [[ ! $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "❌ Version must be in the form X.Y.Z"
    exit 1
fi

echo "📦 Releasing version: $VERSION"

echo "📝 Checking for changes to commit..."
cd "$ROOT_DIR"

if git diff --quiet && git diff --cached --quiet; then
    echo "ℹ️  No changes to commit, proceeding with tag and push..."
else
    echo "📝 Committing and pushing changes to main repository..."
    git add .
    git commit -m "chore: release v$VERSION"
    git push
fi

echo "🏷️  Creating and pushing release tag..."
if git rev-parse "v$VERSION" >/dev/null 2>&1; then
    echo "⚠️  Tag v$VERSION already exists, deleting and recreating..."
    git tag -d "v$VERSION"
    git push origin ":refs/tags/v$VERSION" || true
fi

git tag "v$VERSION"
git push origin "v$VERSION"

echo "🍺 Updating Homebrew formula..."
cd "$HOMEBREW_TAP_DIR"
./update-sha.sh "v$VERSION"

echo "📝 Committing and pushing formula update..."
if git diff --quiet && git diff --cached --quiet; then
    echo "ℹ️  No changes detected in formula."
else
    git add li.rb
    git commit -m "li: update to v$VERSION"
    git push
fi

echo "✅ Release complete! Users can now run:"
echo "   brew update"
echo "   brew upgrade li"
echo ""
echo "🎉 li version $VERSION is now available via Homebrew!"
