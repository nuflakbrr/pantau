#!/usr/bin/env bash
# Build a release .zip for GitHub Releases — the format updater.rs expects
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
rm -f "$OUT"
ditto -c -k --keepParent "$APP" "$OUT"
rm -rf "$STAGE"

echo "Built $OUT — upload this as a release asset tagged v${VERSION} on https://github.com/nuflakbrr/pantau/releases"
