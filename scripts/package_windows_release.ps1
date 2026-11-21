$ErrorActionPreference = 'Stop'

$desktop = 'apps/desktop/src-tauri'
$version = (Select-String -Path "$desktop/Cargo.toml" -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
$outputDir = "$desktop/target/release/bundle/nsis"
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
$app = (Resolve-Path "$desktop/target/release/jarvis-desktop.exe").Path
$core = (Resolve-Path "$desktop/resources/jarvis-core.exe").Path
$resources = (Resolve-Path "$desktop/resources").Path
$output = Join-Path (Resolve-Path $outputDir).Path "JARVIS_${version}_x64-setup.exe"
$makensis = (Get-Command makensis.exe -ErrorAction Stop).Source

& $makensis `
  "/DAPP_VERSION=$version" `
  "/DAPP_EXE=$app" `
  "/DCORE_EXE=$core" `
  "/DRESOURCE_DIR=$resources" `
  "/DOUTPUT=$output" `
  'apps/desktop/installer/windows.nsi'
if ($LASTEXITCODE -ne 0) {
  throw "NSIS non è riuscito a creare il setup (codice $LASTEXITCODE)"
}
if (-not (Test-Path -LiteralPath $output)) {
  throw "Setup Windows non trovato: $output"
}
