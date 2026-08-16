#!/bin/sh
# Double-click this file after opening the DMG.
# It installs DSH Desktop.app to /Applications, replaces any old copy,
# and removes the macOS quarantine flag so Gatekeeper doesn't interfere.
set -e
cd "$(dirname "$0")"
SRC="DSH Desktop.app"
DEST="/Applications/DSH Desktop.app"

echo "Quitting DSH Desktop if running..."
osascript -e 'tell application "DSH Desktop" to quit' >/dev/null 2>&1 || true
sleep 1

echo "Installing to $DEST ..."
rm -rf "$DEST"
cp -R "$SRC" "$DEST"
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true
xattr -dr com.apple.provenance "$DEST" 2>/dev/null || true

echo ""
echo "Installed."
echo "Note: on FIRST launch macOS may ask ONCE for access to removable volumes"
echo "(or Full Disk Access). Click Allow / grant it — with the stable signature"
echo "this permission now persists across every future update."
echo ""
open -R "$DEST"
