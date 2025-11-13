#!/bin/bash

# Script to update li and publish new Homebrew release
set -e

echo "🚀 Starting li release process..."

# Get version from argument or use default
VERSION=${1:-"0.1.5"}
echo "📦 Releasing version: $VERSION"

# 1. Commit and push changes to main repo (if there are changes)
echo "📝 Checking for changes to commit..."
cd /Users/matthew/bitrift/li

# Check if there are any changes to commit
if git diff --quiet && git diff --cached --quiet; then
    echo "ℹ️  No changes to commit, proceeding with tag and push..."
else
    echo "📝 Committing and pushing changes to main repository..."
    git add .
    git commit -m "feat: add AI intelligence mode for command output explanation

- Add -i/--intelligence flag to execute commands and explain outputs
- Implement handle_intelligence function with command execution and AI analysis
- Add comprehensive error handling for failed commands
- Update welcome message with intelligence examples
- Add detailed intelligence section to README with use cases and examples
- Update command options documentation
- Support both short (-i) and long (--intelligence) flag forms
- Provide human-friendly explanations with insights, warnings, and practical understanding"
    git push
fi

# 2. Create and push release tag
echo "🏷️  Creating and pushing release tag..."
if git rev-parse "v$VERSION" >/dev/null 2>&1; then
    echo "⚠️  Tag v$VERSION already exists, deleting and recreating..."
    git tag -d "v$VERSION"
    git push origin ":refs/tags/v$VERSION" || true
fi
git tag "v$VERSION"
git push origin "v$VERSION"

# 3. Update Homebrew formula
echo "🍺 Updating Homebrew formula..."
cd /Users/matthew/bitrift/homebrew-li
./update-sha.sh "v$VERSION"

# 4. Commit and push formula update
echo "📝 Committing and pushing formula update..."
git add li.rb
git commit -m "Update to v$VERSION"
git push

echo "✅ Release complete! Users can now run:"
echo "   brew update"
echo "   brew upgrade li"
echo ""
echo "🎉 li version $VERSION is now available via Homebrew!"
