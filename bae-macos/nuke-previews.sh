#!/usr/bin/env bash
set -euo pipefail

# Nukes all Xcode preview/build caches. After running this, restart
# Xcode and hit Cmd+B before trying to use previews (they need built
# artifacts to render from).

echo "Deleting preview simulators..."
xcrun simctl --set previews delete all 2>/dev/null || true

echo "Deleting preview cache..."
rm -rf ~/Library/Developer/Xcode/UserData/Previews

echo "Deleting derived data..."
rm -rf ~/Library/Developer/Xcode/DerivedData

echo "Done. Restart Xcode, then Cmd+B."
