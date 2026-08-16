#!/bin/sh
# One-time: export the local "DSH Local Codesign" identity as a .p12 for
# GitHub Actions, so CI-built DMGs carry the SAME stable signature as local
# builds and macOS stops re-prompting for permissions on every install.
#
# Usage: tools/export-signing-identity.sh [output.p12]
# Then add two repository secrets:
#   MACOS_SIGNING_P12      = base64 contents of the .b64 file
#   MACOS_SIGNING_PASSWORD = the p12 password you choose here
set -e

NAME="DSH Local Codesign"
OUT="${1:-dsh-codesign.p12}"

LINE=$(security find-identity -v -p codesigning | grep "\"$NAME\"" | head -1)
[ -n "$LINE" ] || { echo "Identity '$NAME' not found. Run tools/make-signing-identity.sh first." >&2; exit 1; }
HASH=$(printf '%s\n' "$LINE" | awk '{print $2}')

printf "Choose a p12 password (min 4 chars): "
stty -echo; read -r PASS; printf "\nRepeat password: "; read -r PASS2; stty echo; printf "\n"
[ "$PASS" = "$PASS2" ] || { echo "Passwords do not match." >&2; exit 1; }
[ ${#PASS} -ge 4 ] || { echo "Password too short." >&2; exit 1; }

security export -k "$HOME/Library/Keychains/login.keychain-db" \
  -t identities -f pkcs12 -P "$PASS" -o "$OUT" "$HASH"
base64 -i "$OUT" -o "$OUT.b64"

echo ""
echo "Created: $OUT and $OUT.b64"
echo "Next (repo -> Settings -> Secrets and variables -> Actions):"
echo "  MACOS_SIGNING_P12       <- full contents of $OUT.b64"
echo "  MACOS_SIGNING_PASSWORD  <- the password you just entered"
echo "After that, every CI DMG is signed with the same identity as local builds."
