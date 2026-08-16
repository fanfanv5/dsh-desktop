#!/bin/sh
# Sign a finished macOS .app bundle with the best identity available.
#
# Why: an unsigned/ad-hoc app gets a NEW code identity on every rebuild, so
# macOS TCC (Full Disk Access, Removable Volumes) treats each build as an
# unknown app and re-prompts. Signing with a stable identity fixes that.
#
# Identity resolution order:
#   1. $DSH_CODESIGN_IDENTITY if set
#   2. the self-signed "DSH Local Codesign" cert (tools/make-signing-identity.sh)
#   3. ad-hoc with a FIXED identifier (last resort; grants will not survive rebuilds)
#
# Usage: tools/codesign-app.sh <path-to.app> [bundle-id]
set -e

APP="$1"
BUNDLE_ID="${2:-com.dshdesktop.app}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENTITLEMENTS="$ROOT/assets/macos-entitlements.plist"

identity="${DSH_CODESIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
  # Use find-identity WITHOUT -v so untrusted-but-usable local certs are found too.
  if security find-identity -p codesigning 2>/dev/null | grep -q '"DSH Local Codesign"'; then
    identity="DSH Local Codesign"
  fi
fi

if [ -n "$identity" ]; then
  echo "codesign: signing '$APP' with identity '$identity'"
  codesign --force --identifier "$BUNDLE_ID" \
    --entitlements "$ENTITLEMENTS" \
    --sign "$identity" "$APP"
else
  echo "codesign: WARNING no stable identity found; falling back to ad-hoc." >&2
  echo "         TCC grants (Full Disk Access / Removable Volumes) will be lost on" >&2
  echo "         every rebuild. Create one once with: tools/make-signing-identity.sh" >&2
  codesign --force --identifier "$BUNDLE_ID" \
    --entitlements "$ENTITLEMENTS" \
    --sign - "$APP"
fi

codesign --verify --strict "$APP"
echo "codesign: OK ($(codesign -dv "$APP" 2>&1 | sed -n 's/^Identifier=//p'))"
