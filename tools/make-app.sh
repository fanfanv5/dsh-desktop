#!/bin/sh
# Build and package "DSH Desktop.app" for local use (single-arch, host).
# CI (release.yml) builds the universal variant with the same layout.
# Usage: tools/make-app.sh [output-dir]
set -e
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT="${1:-dist}"
APP="$OUT/DSH Desktop.app"

cargo build --release
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
COPYFILE_DISABLE=1 cp target/release/dsh-desktop "$APP/Contents/MacOS/dsh-desktop"
COPYFILE_DISABLE=1 cp assets/dsh-desktop.icns "$APP/Contents/Resources/dsh-desktop.icns"
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>DSH Desktop</string>
  <key>CFBundleDisplayName</key><string>DSH Desktop</string>
  <key>CFBundleIdentifier</key><string>com.dshdesktop.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>dsh-desktop</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>dsh-desktop.icns</string>
  <key>LSMinimumSystemVersion</key><string>10.13</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
touch "$APP"

# Sign with a stable identity so macOS keeps TCC grants (Full Disk Access,
# Removable Volumes) across rebuilds. Without this every rebuild is a "new"
# app and macOS re-prompts for external-drive access.
tools/codesign-app.sh "$APP" com.dshdesktop.app

echo "$APP"
