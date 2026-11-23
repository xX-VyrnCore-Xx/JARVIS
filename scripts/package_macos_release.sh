#!/usr/bin/env bash
set -euo pipefail

version="${1:?Uso: package_macos_release.sh VERSION}"
desktop="apps/desktop/src-tauri"
bundle="$desktop/target/release/bundle"
app="$bundle/macos/JARVIS.app"
contents="$app/Contents"
resources="$contents/Resources"
temporary_dir=$(mktemp -d "${RUNNER_TEMP:-/tmp}/jarvis-dmg.XXXXXX")
temporary_dmg="$temporary_dir/JARVIS_${version}_layout.dmg"
mountpoint="$temporary_dir/mount"

cleanup_dmg_work() {
  if hdiutil info | grep -Fq "$mountpoint"; then
    hdiutil detach "$mountpoint" >/dev/null 2>&1 ||
      hdiutil detach -force "$mountpoint" >/dev/null 2>&1 || true
  fi
  rm -f "$temporary_dmg"
  rmdir "$mountpoint" "$temporary_dir" 2>/dev/null || true
}
trap cleanup_dmg_work EXIT

detach_dmg_mount() {
  local target="$1"
  local attempt
  for attempt in 1 2 3; do
    if hdiutil detach "$target"; then
      return 0
    fi
    if (( attempt < 3 )); then
      sleep 2
    fi
  done

  echo "Espulsione normale non riuscita; forzo lo smontaggio del DMG temporaneo."
  hdiutil detach -force "$target"
}

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
    cleanup_dmg_work
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
cp apps/desktop/installer/macos/Installazione.txt "$stage/Installazione.txt"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
mkdir -p "$mountpoint"
echo "Creazione del DMG modificabile per impostare la finestra di installazione."
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDRW "$temporary_dmg"
hdiutil attach -nobrowse -readwrite -mountpoint "$mountpoint" "$temporary_dmg"
JARVIS_DMG_MOUNT="$mountpoint" osascript <<'APPLESCRIPT'
on run
  set dmgFolder to POSIX file (system attribute "JARVIS_DMG_MOUNT") as alias
  tell application "Finder"
    open dmgFolder
    set dmgWindow to front window
    set target of dmgWindow to dmgFolder
    set current view of dmgWindow to icon view
    set toolbar visible of dmgWindow to false
    set statusbar visible of dmgWindow to false
    set bounds of dmgWindow to {80, 100, 1040, 680}
    set viewOptions to icon view options of dmgWindow
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 128
    set text size of viewOptions to 14
    set position of item "JARVIS.app" of dmgWindow to {220, 245}
    set position of item "Applications" of dmgWindow to {780, 245}
    set position of item "Installazione.txt" of dmgWindow to {500, 425}
    close dmgWindow
    delay 2
  end tell
end run
APPLESCRIPT
detach_dmg_mount "$mountpoint"
echo "Compressione del DMG con il layout Finder di installazione."
hdiutil convert "$temporary_dmg" -ov -format UDZO -o "$dmg"
rm "$temporary_dmg"
if [[ "$signing_identity" != - ]]; then
  echo "Firma del DMG con il certificato configurato."
  codesign --force --timestamp=none --verbose=2 --sign "$signing_identity" "$dmg"
  echo "Verifica della firma del DMG."
  codesign --verify --strict --verbose=2 "$dmg"
  trap cleanup EXIT
fi
