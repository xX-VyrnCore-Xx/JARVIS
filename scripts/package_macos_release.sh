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

umask 077
signing_dir=$(mktemp -d)
keychain="$signing_dir/jarvis.keychain-db"
trap 'security delete-keychain "$keychain" >/dev/null 2>&1 || true; rm -rf -- "$signing_dir"' EXIT
security create-keychain -p "" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "" "$keychain"

signing_identity=""
if [[ -n "${JARVIS_MACOS_SIGNING_P12_BASE64:-}" && -n "${JARVIS_MACOS_SIGNING_PASSWORD:-}" ]]; then
  certificate="$signing_dir/secret-certificate.p12"
  if printf '%s' "$JARVIS_MACOS_SIGNING_P12_BASE64" | base64 -D > "$certificate" &&
     openssl pkcs12 -in "$certificate" -passin env:JARVIS_MACOS_SIGNING_PASSWORD -noout >/dev/null 2>&1 &&
     security import "$certificate" -k "$keychain" -P "$JARVIS_MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign >/dev/null 2>&1; then
    echo "Uso il certificato macOS configurato nei Secrets."
  else
    echo "::warning::Il certificato macOS dei Secrets non è utilizzabile; creo un certificato temporaneo."
  fi
fi

if [[ -z "$(security find-identity -v -p codesigning "$keychain" 2>/dev/null)" ]]; then
  openssl req -x509 -newkey rsa:3072 -sha256 -nodes \
    -keyout "$signing_dir/certificate.key" \
    -out "$signing_dir/certificate.crt" \
    -days 3650 \
    -subj "/CN=JARVIS Private Code Signing" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=codeSigning" >/dev/null 2>&1
  fallback_password=$(openssl rand -hex 32)
  openssl pkcs12 -export \
    -out "$signing_dir/fallback-certificate.p12" \
    -inkey "$signing_dir/certificate.key" \
    -in "$signing_dir/certificate.crt" \
    -name "JARVIS Private Code Signing" \
    -passout "pass:$fallback_password" >/dev/null 2>&1
  security import "$signing_dir/fallback-certificate.p12" -k "$keychain" -P "$fallback_password" -T /usr/bin/codesign
  unset fallback_password
fi

security set-key-partition-list -S apple-tool:,apple: -s -k "" "$keychain"
security list-keychains -d user -s "$keychain"
signing_identity=$(security find-identity -v -p codesigning "$keychain" |
  awk '/^[[:space:]]*[0-9]+\)/ { print $2; exit }')
if [[ ! "$signing_identity" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "Il certificato di firma macOS non ha generato un'identità valida." >&2
  exit 1
fi

codesign --force --deep --sign "$signing_identity" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

stage="$bundle/dmg/JARVIS"
mkdir -p "$stage" "$bundle/dmg"
cp -R "$app" "$stage/JARVIS.app"
ln -s /Applications "$stage/Applications"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDZO "$dmg"
codesign --force --sign "$signing_identity" "$dmg"
codesign --verify --strict "$dmg"
