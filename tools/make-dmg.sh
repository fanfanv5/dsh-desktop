#!/bin/sh
# Build a local single-arch DMG from dist/DSH Desktop.app, with the same
# layout as CI (app + "Install DSH Desktop.command").
# Usage: tools/make-dmg.sh [output.dmg]
set -e
cd "$(dirname "$0")/.."

APP="dist/DSH Desktop.app"
[ -d "$APP" ] || { echo "run tools/make-app.sh first" >&2; exit 1; }

ARCH=$(uname -m)
OUT="${1:-dist/DSH-Desktop-macos-$ARCH.dmg}"

STAGE=$(mktemp -d "${TMPDIR:-/tmp}/dsh-dmg.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
COPYFILE_DISABLE=1 cp -R "$APP" "$STAGE/"
COPYFILE_DISABLE=1 cp "installer/Install DSH Desktop.command" "$STAGE/"
rm -f "$OUT"
hdiutil create -volname "DSH Desktop" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
echo "$OUT"
