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
if [[ "${JARVIS_REQUIRE_DESKTOP_SIGNATURE:-false}" == true ]]; then
  : "${JARVIS_MACOS_SIGNING_P12_BASE64:?Secret P12 macOS mancante}"
  signing_dir=$(mktemp -d)
  keychain="$signing_dir/jarvis.keychain-db"
  certificate="$signing_dir/certificate.p12"
  old_keychains=()
  while IFS= read -r path; do
    old_keychains+=("$path")
  done < <(security list-keychains -d user | sed -E 's/^ *"//; s/"$//')
  cleanup() {
    echo "Rimozione del portachiavi di firma temporaneo."
    security list-keychains -d user -s ${old_keychains[@]+"${old_keychains[@]}"} >/dev/null 2>&1 || true
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
  export JARVIS_MACOS_SIGNING_PASSWORD="${JARVIS_MACOS_SIGNING_PASSWORD:-}"
  import_p12() {
    security import "$1" -f pkcs12 -k "$keychain" -P "$JARVIS_MACOS_SIGNING_PASSWORD" -T /usr/bin/codesign >/dev/null 2>&1
  }
  echo "Importazione del P12 macOS nel portachiavi temporaneo."
  if ! import_p12 "$certificate"; then
    # Apple Security may reject the encryption scheme of a valid OpenSSL export.
    # Rewrap the same identity via a pipe so the plaintext private key stays off disk.
    normalized="$signing_dir/compatible.p12"
    read_options=()
    if ! openssl pkcs12 -in "$certificate" -passin env:JARVIS_MACOS_SIGNING_PASSWORD -noout >/dev/null 2>&1; then
      read_options=(-legacy)
    fi
    public_chain="$signing_dir/chain.pem"
    if ! openssl pkcs12 ${read_options[@]+"${read_options[@]}"} -in "$certificate" -passin env:JARVIS_MACOS_SIGNING_PASSWORD -nokeys -out "$public_chain" 2>/dev/null ||
       ! (openssl pkcs12 ${read_options[@]+"${read_options[@]}"} -in "$certificate" -passin env:JARVIS_MACOS_SIGNING_PASSWORD -nocerts -nodes |
          openssl pkcs12 -export -in "$public_chain" -inkey /dev/stdin -out "$normalized" \
            -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
            -passout env:JARVIS_MACOS_SIGNING_PASSWORD) 2>/dev/null || ! import_p12 "$normalized"; then
      echo "::error::Importazione P12 macOS non riuscita anche dopo la normalizzazione. Verificare il contenitore e la password configurati."
      exit 1
    fi
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
    if [[ "${RUNNER_ENVIRONMENT:-}" == github-hosted ]] && [[ -s "$leaf" ]] &&
       [[ "$(openssl x509 -in "$leaf" -noout -subject -nameopt RFC2253 | sed 's/^subject= *//')" == "$(openssl x509 -in "$leaf" -noout -issuer -nameopt RFC2253 | sed 's/^issuer= *//')" ]] &&
       openssl verify -check_ss_sig -CAfile "$leaf" "$leaf" >/dev/null 2>&1; then
      echo "Configurazione della fiducia per il certificato autofirmato sul runner."
      sudo -n security add-trusted-cert -d -r trustRoot -p codeSign -k /Library/Keychains/System.keychain "$leaf"
      signing_identity=$(identity)
    fi
  fi
  if [[ ! "$signing_identity" =~ ^[[:xdigit:]]{40}$ ]]; then
    echo "::error::Il P12 macOS configurato non contiene un’identità valida per la firma del codice."
    exit 1
  fi
  echo "Configurazione dell’accesso alla chiave per codesign."
  security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain" >/dev/null
  # Register the temporary keychain, as required for codesign to build the
  # identity's trust chain on GitHub-hosted macOS runners.
  security list-keychains -d user -s "$keychain" ${old_keychains[@]+"${old_keychains[@]}"}
  echo "Identità macOS configurata selezionata."
fi

echo "Firma e verifica dell’app macOS."
codesign --force --deep --timestamp=none --sign "$signing_identity" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

stage="$bundle/dmg/JARVIS"
mkdir -p "$stage" "$bundle/dmg"
cp -R "$app" "$stage/JARVIS.app"
ln -s /Applications "$stage/Applications"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
echo "Creazione del DMG."
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDZO "$dmg"
if [[ "$signing_identity" != - ]]; then
  echo "Firma del DMG con il certificato configurato."
  codesign --force --timestamp=none --verbose=2 --sign "$signing_identity" "$dmg"
  echo "Verifica della firma del DMG."
  codesign --verify --strict --verbose=2 "$dmg"
fi
