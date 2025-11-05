#!/bin/bash

# Script to update li and publish new Homebrew release
set -e

echo "🚀 Starting li release process..."

# Get version from argument or use default
VERSION=${1:-"0.1.1"}
echo "📦 Releasing version: $VERSION"

# 1. Commit and push changes to main repo
echo "📝 Committing and pushing changes to main repository..."
cd /Users/matthew/bitrift/li
git add .
git commit -m "feat: update CLI to use --config flags instead of config subcommand

- Replace 'li config' with 'li --config' command structure  
- Add --api-key, --timeout, --max-tokens, --classifier-model, --planner-model flags
- Update welcome message and README documentation
- Improve configuration handling to preserve existing settings"
git push

# 2. Create and push release tag
echo "🏷️  Creating and pushing release tag..."
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
