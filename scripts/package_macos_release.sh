#!/usr/bin/env bash
set -euo pipefail

version="${1:?Uso: package_macos_release.sh VERSION}"
desktop="apps/desktop/src-tauri"
bundle="$desktop/target/release/bundle"
app="$bundle/macos/JARVIS.app"
contents="$app/Contents"
resources="$contents/Resources"

mkdir -p "$contents/MacOS" "$resources"
cp "$desktop/target/release/jarvis-desktop" "$contents/MacOS/jarvis-desktop"
cp -R "$desktop/resources/." "$resources/"
cp "$desktop/icons/icon.icns" "$resources/icon.icns"
sed "s/@VERSION@/$version/g" apps/desktop/installer/macos/Info.plist \
  > "$contents/Info.plist"

if [[ "${NATIVE_SIGNING:-false}" == "true" ]]; then
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
  keychain="$signing_dir/jarvis.keychain-db"
  trap 'security delete-keychain "$keychain" >/dev/null 2>&1 || true; rm -rf -- "$signing_dir"' EXIT
  security create-keychain -p "" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "" "$keychain"

  certificate="$signing_dir/certificate.p12"
  printf '%s' "$APPLE_CERTIFICATE" | base64 -D > "$certificate"
  unset APPLE_CERTIFICATE
  security import "$certificate" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
  security set-key-partition-list -S apple-tool:,apple: -s -k "" "$keychain"
  security list-keychains -d user -s "$keychain"

  codesign --force --deep --options runtime --timestamp \
    --sign "$APPLE_SIGNING_IDENTITY" "$app"
  codesign --verify --deep --strict --verbose=2 "$app"
else
  codesign --force --deep --sign - "$app"
fi

stage="$bundle/dmg/JARVIS"
mkdir -p "$stage" "$bundle/dmg"
cp -R "$app" "$stage/JARVIS.app"
ln -s /Applications "$stage/Applications"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDZO "$dmg"

if [[ "${NATIVE_SIGNING:-false}" == "true" ]]; then
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$dmg"
  codesign --verify --strict "$dmg"
  codesign -dv --verbose=4 "$dmg" 2>&1 | grep -F "Authority=$APPLE_SIGNING_IDENTITY"

  api_key="$signing_dir/AuthKey.p8"
  printf '%s' "$JARVIS_APPLE_API_KEY_BASE64" | base64 -D > "$api_key"
  unset JARVIS_APPLE_API_KEY_BASE64
  xcrun notarytool submit "$dmg" --key "$api_key" --key-id "$APPLE_API_KEY" \
    --issuer "$APPLE_API_ISSUER" --wait --timeout 20m --output-format json \
    > "$signing_dir/notary.json"
  jq -e '.status == "Accepted"' "$signing_dir/notary.json" > /dev/null
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
fi
