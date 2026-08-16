#!/bin/sh
# CI: import the signing identity from MACOS_SIGNING_P12 / MACOS_SIGNING_PASSWORD.
# If the secrets are absent, exits 0 — tools/codesign-app.sh then falls back to
# ad-hoc signing and macOS will re-prompt for permissions on every install.
set -e

if [ -z "$MACOS_SIGNING_P12" ]; then
  echo "::warning::MACOS_SIGNING_P12 secret not set; DMG will be ad-hoc signed (macOS re-prompts for TCC permissions on each install)."
  exit 0
fi
[ -n "$MACOS_SIGNING_PASSWORD" ] || { echo "MACOS_SIGNING_PASSWORD secret missing" >&2; exit 1; }
[ -n "$RUNNER_TEMP" ] || { echo "not a CI runner (RUNNER_TEMP unset)" >&2; exit 1; }

KC_PW=$(openssl rand -hex 12)
KC="$RUNNER_TEMP/dsh-signing.keychain-db"
rm -f "$KC"
security create-keychain -p "$KC_PW" "$KC"
security unlock-keychain -p "$KC_PW" "$KC"

printf '%s' "$MACOS_SIGNING_P12" | base64 -d > "$RUNNER_TEMP/cert.p12"
security import "$RUNNER_TEMP/cert.p12" -k "$KC" -P "$MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$KC_PW" "$KC" >/dev/null

# Put our keychain first in the search list so codesign finds the identity.
security list-keychain -d user -s "$KC" $(security list-keychain -d user | tr '\n' ' ')
echo "Signing identity imported into $KC"
