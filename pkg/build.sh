#!/usr/bin/env bash
# Build pantau-app in release mode, assemble it into Pantau.app in a temp
# dir (no persistent staging copy left inside the project — Spotlight was
# indexing that as a second "Pantau" result alongside /Applications/Pantau.app),
# ad-hoc codesign it, and install it to /Applications.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

STAGE="$(mktemp -d)/Pantau.app"
mkdir -p "$STAGE/Contents/MacOS" "$STAGE/Contents/Resources"
cp target/release/pantau-app "$STAGE/Contents/MacOS/pantau-app"
cp pkg/Info.plist "$STAGE/Contents/Info.plist"
cp assets/AppIcon.icns "$STAGE/Contents/Resources/AppIcon.icns"
codesign --force --deep --sign - "$STAGE"

pkill -x pantau-app 2>/dev/null || true

rm -rf "/Applications/Pantau.app"
cp -R "$STAGE" /Applications/
rm -rf "$(dirname "$STAGE")"
open /Applications/Pantau.app

echo "Installed to /Applications/Pantau.app and launched."
