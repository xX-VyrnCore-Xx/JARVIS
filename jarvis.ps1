# Avvio di JARVIS su Windows (PowerShell). Equivalente di jarvis.sh.
# Il file resta solo ASCII: Windows PowerShell 5.1 legge gli script senza BOM come ANSI (cp1252).
#
# Uso: .\jarvis.ps1               interfaccia nel browser su http://localhost:8765
#      .\jarvis.ps1 desktop       app desktop (Tauri)
#      .\jarvis.ps1 voice-setup   motori vocali locali e modelli (una volta)
#      .\jarvis.ps1 doctor        controlla i prerequisiti
#      .\jarvis.ps1 backup        backup cifrato dei dati
#      .\jarvis.ps1 restore FILE  ripristina un backup (con JARVIS chiuso)
#      .\jarvis.ps1 rollback      riporta i dati alla copia fatta prima dell'ultimo aggiornamento
# In una release scaricata il Core e' gia' compilato (bin\jarvis.exe) e Rust non serve.
# Se PowerShell blocca gli script, usa jarvis.cmd (doppio clic) che lo avvia con -ExecutionPolicy Bypass.

param([string]$Command = "", [string]$Target = "")
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Port = if ($env:JARVIS_PORT) { $env:JARVIS_PORT } else { "8765" }
$Venv = Join-Path $Root ".venv"
$VenvPython = Join-Path $Venv "Scripts\python.exe"
$Prebuilt = (Test-Path (Join-Path $Root "bin\jarvis.exe")) -and -not (Test-Path (Join-Path $Root "core\Cargo.toml"))
$CoreExe = if ($Prebuilt) { Join-Path $Root "bin\jarvis.exe" } else { Join-Path $Root "core\target\release\jarvis.exe" }

function Fail($msg) { Write-Host "[x] $msg" -ForegroundColor Red; exit 1 }
function Ok($msg) { Write-Host "[ok] $msg" -ForegroundColor Green }
function Warn($msg) { Write-Host "! $msg" -ForegroundColor Yellow }

function Find-Python {
    foreach ($candidate in @(@("py", "-3"), @("python"), @("python3"))) {
        $exe = $candidate[0]
        if (Get-Command $exe -ErrorAction SilentlyContinue) {
            $extra = @()
            if ($candidate.Length -gt 1) { $extra = $candidate[1..($candidate.Length - 1)] }
            & $exe @extra -c "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)" 2>$null
            if ($LASTEXITCODE -eq 0) { return , $candidate }
        }
    }
    return $null
}

function Invoke-Doctor {
    $problems = 0
    Write-Host "JARVIS - controllo dei prerequisiti (Windows)"
    $py = Find-Python
    if ($py) { Ok "Python 3.10+ ($($py -join ' '))" } else { Warn "Python 3.10+ mancante: https://www.python.org/downloads/ (spunta 'Add to PATH')"; $problems++ }
    if ($Prebuilt) { Ok "Core precompilato (release): Rust non serve" }
    elseif (Get-Command cargo -ErrorAction SilentlyContinue) { Ok "Rust $((cargo --version).Split(' ')[1])" } else { Warn "Rust mancante: https://rustup.rs/ (installa anche 'Desktop development with C++' se richiesto)"; $problems++ }
    if (Test-Path (Join-Path $Root "core\target\release\jarvis.exe")) { Ok "Core gia' compilato" } else { Write-Host "- Core non ancora compilato (succede al primo avvio)" }
    $busy = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    if ($busy) { Warn "La porta $Port e' occupata: imposta `$env:JARVIS_PORT=8766"; $problems++ } else { Ok "Porta $Port libera" }
    if ($problems -eq 0) { Ok "Tutto pronto: .\jarvis.ps1" } else { Write-Host "Problemi trovati: $problems" }
    return $problems
}

if ($Command -eq "doctor") { exit (Invoke-Doctor) }

$py = Find-Python
if (-not $py) { Invoke-Doctor | Out-Null; Fail "Serve Python 3.10 o successivo" }
if (-not $Prebuilt -and -not (Get-Command cargo -ErrorAction SilentlyContinue)) { Invoke-Doctor | Out-Null; Fail "Rust non trovato: installalo da https://rustup.rs/ e riapri il terminale" }

if ((Test-Path $Venv) -and -not (Test-Path $VenvPython)) { Warn "Ambiente .venv incompleto: lo ricreo"; Remove-Item -Recurse -Force $Venv }
if (-not (Test-Path $VenvPython)) {
    Write-Host "-> Creo l'ambiente Python in .venv"
    $pyArgs = @(); if ($py.Length -gt 1) { $pyArgs = $py[1..($py.Length - 1)] }
    & $py[0] @pyArgs -m venv $Venv
    if ($LASTEXITCODE -ne 0) { Fail "Impossibile creare .venv" }
}
Write-Host "-> Dipendenze Python"
& $VenvPython -m pip install -q --disable-pip-version-check -r (Join-Path $Root "tests\contracts\requirements.txt")
if ($LASTEXITCODE -ne 0) { Fail "Installazione delle dipendenze Python non riuscita (serve la rete la prima volta)" }

if ($Prebuilt) { Write-Host "-> Core precompilato" } else {
Push-Location (Join-Path $Root "core")
try {
    if (-not (Test-Path $CoreExe)) { Write-Host "-> Compilo il Core: la prima volta richiede alcuni minuti"; cargo build --release --locked -p jarvis-core }
    else { Write-Host "-> Aggiorno il Core"; cargo build --release --locked -q -p jarvis-core }
    if ($LASTEXITCODE -ne 0) { Fail "Compilazione non riuscita: esegui .\jarvis.ps1 doctor" }
} finally { Pop-Location }
}

$env:JARVIS_PYTHON = $VenvPython
$env:JARVIS_HOME = $Root
$env:JARVIS_PORT = $Port

$Models = if ($env:JARVIS_MODELS_DIR) { $env:JARVIS_MODELS_DIR } else { Join-Path $Root "data\models" }
$VoiceName = if ($env:JARVIS_TTS_VOICE_NAME) { $env:JARVIS_TTS_VOICE_NAME } else { "it_IT-paola-medium" }
if ($Command -eq "voice-setup") {
    & $VenvPython -m pip install -q --disable-pip-version-check -r (Join-Path $Root "ai\requirements-voice.txt")
    New-Item -ItemType Directory -Force -Path (Join-Path $Models "piper") | Out-Null
    $stt = if ($env:JARVIS_STT_MODEL) { $env:JARVIS_STT_MODEL } else { "small" }
    & $VenvPython -c "import sys; from faster_whisper import WhisperModel; WhisperModel(sys.argv[1], device='cpu', compute_type='int8', download_root=sys.argv[2])" $stt $Models
    & $VenvPython -m piper.download_voices $VoiceName --download-dir (Join-Path $Models "piper")
    Ok "Voce pronta. Avvia .\jarvis.ps1"
    exit 0
}
if (Test-Path $Models) {
    $env:JARVIS_MODELS_DIR = $Models
    $voice = Join-Path $Models "piper\$VoiceName.onnx"
    if (-not $env:JARVIS_TTS_VOICE -and (Test-Path $voice)) { $env:JARVIS_TTS_VOICE = $voice }
}

if ($Command -eq "desktop") {
    if ($Prebuilt) { Fail "La release non include l'app desktop: usa .\jarvis.ps1 (interfaccia nel browser) oppure i sorgenti" }
    Push-Location (Join-Path $Root "apps\desktop\src-tauri")
    try { cargo build --release --locked; if ($LASTEXITCODE -ne 0) { Fail "Compilazione dell'app desktop non riuscita" } } finally { Pop-Location }
    & (Join-Path $Root "apps\desktop\src-tauri\target\release\jarvis-desktop.exe")
    exit $LASTEXITCODE
}

if ($Command -eq "backup") { & $CoreExe backup; exit $LASTEXITCODE }
if ($Command -eq "restore") { & $CoreExe restore $Target; exit $LASTEXITCODE }
if ($Command -eq "rollback") { if ($Target) { & $CoreExe rollback $Target } else { & $CoreExe rollback }; exit $LASTEXITCODE }
if ($env:JARVIS_NO_BROWSER -ne "1") {
    Start-Job -ArgumentList $Port -ScriptBlock {
        param($p)
        for ($i = 0; $i -lt 60; $i++) {
            try { Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 "http://127.0.0.1:$p/api/health" | Out-Null; Start-Process "http://localhost:$p/"; break } catch { Start-Sleep 1 }
        }
    } | Out-Null
}
& $CoreExe
exit $LASTEXITCODE
