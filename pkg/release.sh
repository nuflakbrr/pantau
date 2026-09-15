#!/usr/bin/env bash
# Build release .zip and .dmg assets for GitHub Releases — the format updater.rs expects
# to find "Pantau.app" inside after `ditto -x -k` extraction. Version comes
# from Cargo.toml (single source of truth, also embedded in the binary via
# CARGO_PKG_VERSION and compared against Info.plist's CFBundleShortVersionString).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
PLIST_VERSION="$(grep -A1 CFBundleShortVersionString pkg/Info.plist | tail -1 | sed -E 's/.*<string>(.*)<\/string>.*/\1/')"
if [ "$VERSION" != "$PLIST_VERSION" ]; then
	echo "error: Cargo.toml version ($VERSION) != pkg/Info.plist CFBundleShortVersionString ($PLIST_VERSION) — bump both together." >&2
	exit 1
fi

cargo build --release

STAGE="$(mktemp -d)"
APP="$STAGE/Pantau.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/pantau-app "$APP/Contents/MacOS/pantau-app"
cp pkg/Info.plist "$APP/Contents/Info.plist"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
codesign --force --deep --sign - "$APP"

OUT="pkg/Pantau-v${VERSION}.zip"
DMG="pkg/Pantau-v${VERSION}.dmg"
rm -f "$OUT"
rm -f "$DMG"
ditto -c -k --keepParent "$APP" "$OUT"

DMG_STAGE="$(mktemp -d)"
cp -R "$APP" "$DMG_STAGE/Pantau.app"
BACKGROUND="$(mktemp -t pantau-dmg-background).png"
python3 pkg/dmg_background.py "$BACKGROUND"
create-dmg \
  --volname "Pantau ${VERSION}" \
  --background "$BACKGROUND" \
  --window-pos 100 100 \
  --window-size 500 320 \
  --icon-size 72 \
  --icon "Pantau.app" 130 120 \
  --app-drop-link 270 120 \
  "$DMG" "$DMG_STAGE" >/dev/null
rm -f "$BACKGROUND"
rm -rf "$DMG_STAGE"

POSTPROCESS_RW="$(mktemp -t pantau-dmg-postprocess).dmg"
hdiutil convert "$DMG" -format UDRW -o "$POSTPROCESS_RW" >/dev/null
MOUNT_OUTPUT="$(hdiutil attach "$POSTPROCESS_RW" -nobrowse -noautoopen)"
MOUNT_POINT="$(printf '%s\n' "$MOUNT_OUTPUT" | awk '/\/Volumes\// {print substr($0, index($0,$3)); exit}')"
SetFile -a V "$MOUNT_POINT/.background"
chflags hidden "$MOUNT_POINT/.background"
osascript <<EOF
tell application "Finder"
  tell disk "Pantau ${VERSION}"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {100, 100, 500, 320}
    set position of item "Pantau.app" to {130, 90}
    set position of item "Applications" to {270, 90}
    close
    open
    set bounds of container window to {100, 100, 500, 320}
    set position of item "Pantau.app" to {130, 90}
    set position of item "Applications" to {270, 90}
  end tell
end tell
EOF
hdiutil detach "$MOUNT_POINT" >/dev/null
rm -f "$DMG"
hdiutil convert "$POSTPROCESS_RW" -format UDZO -o "$DMG" >/dev/null
rm -f "$POSTPROCESS_RW"
rm -rf "$STAGE"

echo "Built $OUT and $DMG — upload these as release assets tagged v${VERSION} on https://github.com/nuflakbrr/pantau/releases"
