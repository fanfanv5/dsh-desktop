#!/bin/sh
# One-time setup: create a self-signed code-signing identity named
# "DSH Local Codesign" in the login keychain.
#
# After this exists, every tools/make-app.sh build is signed with the SAME
# identity, so macOS remembers Full Disk Access / Removable Volumes grants
# across rebuilds instead of re-prompting.
#
# Optional: export KEYCHAIN_PASSWORD="your-mac-password" to skip the
# "codesign wants to use key" click-through on first signing.
set -e

NAME="DSH Local Codesign"
LOGIN_KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -p codesigning 2>/dev/null | grep -q "\"$NAME\""; then
  echo "Identity '$NAME' already exists; nothing to do."
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

openssl req -newkey rsa:2048 -nodes -keyout "$WORK/key.pem" \
  -x509 -days 36500 -out "$WORK/cert.pem" \
  -subj "/CN=$NAME/O=DSH Local/C=US" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"

security import "$WORK/key.pem" -k "$LOGIN_KEYCHAIN" -T /usr/bin/codesign
security import "$WORK/cert.pem" -k "$LOGIN_KEYCHAIN" -T /usr/bin/codesign

# Best-effort local trust. NOT required: codesign and TCC persistence work
# with an untrusted self-signed cert (verified in practice). add-trusted-cert
# can pop an admin password dialog, so never let it block or fail the setup.
security add-trusted-cert -p codeSign -k "$LOGIN_KEYCHAIN" "$WORK/cert.pem" 2>/dev/null || echo "(local trust settings skipped — not needed for signing)"

if [ -n "$KEYCHAIN_PASSWORD" ]; then
  security set-key-partition-list -S apple-tool:,apple: -s \
    -k "$KEYCHAIN_PASSWORD" "$LOGIN_KEYCHAIN" >/dev/null
  echo "Key access pre-authorized for codesign."
fi

echo ""
echo "Done. Identity '$NAME' created."
echo "Rebuild the app now:  tools/make-app.sh"
echo "(If a dialog asks whether codesign may use the key, click 'Always Allow'.)"
