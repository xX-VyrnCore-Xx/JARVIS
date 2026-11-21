#!/usr/bin/env bash
# Builds the application and disk image with a Developer ID, then notarizes the disk image.
set -euo pipefail

: "${APPLE_CERTIFICATE:?Configurare il certificato Developer ID Application}"
: "${APPLE_CERTIFICATE_PASSWORD:?Configurare la password del certificato}"
: "${APPLE_SIGNING_IDENTITY:?Configurare l’identità di firma}"
: "${APPLE_API_KEY:?Configurare l’identificativo della chiave di notarizzazione}"
: "${APPLE_API_ISSUER:?Configurare l’emittente della chiave di notarizzazione}"
: "${JARVIS_APPLE_API_KEY_BASE64:?Configurare la chiave di notarizzazione}"

case "$APPLE_SIGNING_IDENTITY" in
  "Developer ID Application:"*) ;;
  *) echo "È richiesto un certificato Developer ID Application" >&2; exit 1 ;;
esac

umask 077
signing_dir=$(mktemp -d)
trap 'rm -r -- "$signing_dir"' EXIT
export APPLE_API_KEY_PATH="$signing_dir/AuthKey.p8"
printf '%s' "$JARVIS_APPLE_API_KEY_BASE64" | base64 -D > "$APPLE_API_KEY_PATH"
unset JARVIS_APPLE_API_KEY_BASE64
config=$(jq -c --arg identity "$APPLE_SIGNING_IDENTITY" \
  '.bundle.macOS.signingIdentity = $identity' tauri.bundle.conf.json)

# Tauri signs the nested binaries, app and DMG. Its notarization step staples the app.
if ! CI=false tauri build --config "$config" --bundles dmg; then
  echo "::warning::Build DMG con impaginazione non riuscita, riprovo in modalità CI"
  tauri build --config "$config" --bundles dmg
fi

dmgs=(target/release/bundle/dmg/*.dmg)
[[ ${#dmgs[@]} -eq 1 && -f ${dmgs[0]} ]]
dmg="${dmgs[0]}"
codesign --verify --strict "$dmg"
codesign -dv --verbose=4 "$dmg" 2>&1 | grep -F "Authority=$APPLE_SIGNING_IDENTITY"
xcrun notarytool submit "$dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" --wait --timeout 20m --output-format json > "$signing_dir/notary.json"
jq -e '.status == "Accepted"' "$signing_dir/notary.json" > /dev/null
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
