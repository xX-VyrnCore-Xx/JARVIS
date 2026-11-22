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

stage="$bundle/dmg/JARVIS"
mkdir -p "$stage" "$bundle/dmg"
cp -R "$app" "$stage/JARVIS.app"
ln -s /Applications "$stage/Applications"
dmg="$bundle/dmg/JARVIS_${version}_aarch64.dmg"
hdiutil create -volname JARVIS -srcfolder "$stage" -ov -format UDZO "$dmg"
