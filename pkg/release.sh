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
ln -s /Applications "$DMG_STAGE/Applications"
DMG_RW="$(mktemp -t pantau-dmg).dmg"
hdiutil create -volname "Pantau ${VERSION}" -srcfolder "$DMG_STAGE" -ov -format UDRW "$DMG_RW" >/dev/null
DEVICE="$(hdiutil attach "$DMG_RW" -nobrowse -noautoopen | awk '/\/Volumes\// {print $1; exit}')"
MOUNT_POINT="/Volumes/Pantau ${VERSION}"

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
    update without registering applications
  end tell
end tell
EOF

hdiutil detach "$DEVICE" -force >/dev/null
hdiutil convert "$DMG_RW" -format UDZO -o "$DMG" >/dev/null
rm -f "$DMG_RW"
rm -rf "$DMG_STAGE"
rm -rf "$STAGE"

echo "Built $OUT and $DMG — upload these as release assets tagged v${VERSION} on https://github.com/nuflakbrr/pantau/releases"
