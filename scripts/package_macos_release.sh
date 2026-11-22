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

signing_identity=-
signing_options=()
if [[ "${JARVIS_REQUIRE_DESKTOP_SIGNATURE:-false}" == true ]]; then
  : "${JARVIS_MACOS_SIGNING_P12_BASE64:?Secret P12 macOS mancante}"
  umask 077
  signing_dir=$(mktemp -d)
  keychain="$signing_dir/jarvis.keychain-db"
  certificate="$signing_dir/certificate.p12"
  trusted_leaf=""
  cleanup() {
    if [[ -n "$trusted_leaf" ]]; then sudo security remove-trusted-cert -d "$trusted_leaf" >/dev/null 2>&1 || true; fi
    security delete-keychain "$keychain" >/dev/null 2>&1 || true
    rm -rf -- "$signing_dir"
  }
  trap cleanup EXIT
  keychain_password=$(openssl rand -hex 24)
  echo "::add-mask::$keychain_password"
  security create-keychain -p "$keychain_password" "$keychain"
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain"
  printf '%s' "$JARVIS_MACOS_SIGNING_P12_BASE64" | tr -d '[:space:]' | base64 -D > "$certificate"
  if ! security import "$certificate" -f pkcs12 -k "$keychain" -P "${JARVIS_MACOS_SIGNING_PASSWORD:-}" -T /usr/bin/codesign >/dev/null 2>&1; then
    echo "::error::Importazione P12 macOS non riuscita. Verificare il contenitore e la password configurati."
    exit 1
  fi
  identity() {
    security find-identity -v -p codesigning "$keychain" | awk '$2 ~ /^[[:xdigit:]]+$/ && length($2) == 40 {print $2; exit}'
  }
  signing_identity=$(identity)
  if [[ -z "$signing_identity" ]]; then
    # Trust only a self-signed signing leaf on the disposable runner.
    candidate=$(security find-identity -p codesigning "$keychain" | awk '$2 ~ /^[[:xdigit:]]+$/ && length($2) == 40 {print $2; exit}')
    leaf="$signing_dir/leaf.pem"
    security find-certificate -a -Z -p "$keychain" | awk -v hash="$candidate" '
      /^SHA-1 hash:/ {selected=($NF==hash)}
      selected && /BEGIN CERTIFICATE/ {printing=1}
      printing {print}
      printing && /END CERTIFICATE/ {exit}
    ' > "$leaf"
    if [[ -s "$leaf" ]] &&
       [[ "$(openssl x509 -in "$leaf" -noout -subject -nameopt RFC2253 | sed 's/^subject= *//')" == "$(openssl x509 -in "$leaf" -noout -issuer -nameopt RFC2253 | sed 's/^issuer= *//')" ]] &&
       openssl verify -check_ss_sig -CAfile "$leaf" "$leaf" >/dev/null 2>&1; then
      sudo security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain "$leaf"
      trusted_leaf="$leaf"
      signing_identity=$(identity)
    fi
  fi
  if [[ ! "$signing_identity" =~ ^[[:xdigit:]]{40}$ ]]; then
    echo "::error::Il P12 macOS configurato non contiene un’identità valida per la firma del codice."
    exit 1
  fi
  security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
  signing_options=(--keychain "$keychain")
  echo "Identità macOS configurata selezionata."
fi

codesign --force --deep --timestamp=none "${signing_options[@]}" --sign "$signing_identity" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

stage="$bundle/dmg/JARVIS"
mkdir -p "$stage" "$bundle/dmg"
cp -R "$app" "$stage/JARVIS.app"
ln -s /Applications "$stage/Applications"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDZO "$dmg"
if [[ "$signing_identity" != - ]]; then
  codesign --force --timestamp=none "${signing_options[@]}" --sign "$signing_identity" "$dmg"
  codesign --verify --strict "$dmg"
fi
