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
  : "${JARVIS_MACOS_SIGNING_P12_BASE64:?Configurare il certificato autofirmato macOS}"
  : "${JARVIS_MACOS_SIGNING_PASSWORD:?Configurare la password del certificato macOS}"
  : "${JARVIS_MACOS_SIGNING_IDENTITY:?Configurare l’impronta SHA-1 del certificato}"

  umask 077
  signing_dir=$(mktemp -d)
  keychain="$signing_dir/jarvis.keychain-db"
  trap 'security delete-keychain "$keychain" >/dev/null 2>&1 || true; rm -rf -- "$signing_dir"' EXIT
  security create-keychain -p "" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "" "$keychain"

  certificate="$signing_dir/certificate.p12"
  printf '%s' "$JARVIS_MACOS_SIGNING_P12_BASE64" | base64 -D > "$certificate"
  unset JARVIS_MACOS_SIGNING_P12_BASE64
  security import "$certificate" -k "$keychain" -P "$JARVIS_MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign
  unset JARVIS_MACOS_SIGNING_PASSWORD
  security set-key-partition-list -S apple-tool:,apple: -s -k "" "$keychain"
  security list-keychains -d user -s "$keychain"

  codesign --force --deep --sign "$JARVIS_MACOS_SIGNING_IDENTITY" "$app"
  codesign --verify --deep --strict --verbose=2 "$app"
  codesign -dv --verbose=4 "$app" 2>&1 | grep -F "Authority=JARVIS Private Code Signing"
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
  codesign --force --sign "$JARVIS_MACOS_SIGNING_IDENTITY" "$dmg"
  codesign --verify --strict "$dmg"
  codesign -dv --verbose=4 "$dmg" 2>&1 | grep -F "Authority=JARVIS Private Code Signing"
fi
