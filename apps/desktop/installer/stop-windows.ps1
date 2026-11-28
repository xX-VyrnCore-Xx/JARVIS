param([Parameter(Mandatory = $true)][string] $InstallDir)

$ErrorActionPreference = 'Stop'
try {
    $root = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
    $binaries = @(
        (Join-Path $root 'jarvis-desktop.exe'),
        (Join-Path $root 'jarvis-core.exe'),
        (Join-Path $root 'resources\jarvis-core.exe'),
        (Join-Path $root 'jarvis.exe')
    )
    $runtime = (Join-Path $root 'resources') + '\'
    for ($attempt = 0; $attempt -lt 3; $attempt++) {
        $running = @(Get-CimInstance Win32_Process -Filter "Name = 'jarvis-desktop.exe' OR Name = 'jarvis.exe' OR Name = 'jarvis-core.exe' OR Name = 'python.exe' OR Name = 'pythonw.exe'" |
            Where-Object {
                $_.ExecutablePath -and (
                    $_.ExecutablePath -in $binaries -or
                    ($_.Name -in @('python.exe', 'pythonw.exe') -and
                     $_.ExecutablePath.StartsWith($runtime, [StringComparison]::OrdinalIgnoreCase))
                )
            } | Sort-Object { $_.Name -ne 'jarvis-desktop.exe' })
        if ($running.Count -eq 0) { exit 0 }
        foreach ($process in $running) {
            $live = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
            if ($live -and $live.Path -eq $process.ExecutablePath) {
                try {
                    $live.Kill()
                    $null = $live.WaitForExit(2000)
                } catch {
                    if (-not $live.HasExited) { throw }
                }
            }
        }
    }
    throw 'Alcuni processi JARVIS non si sono arrestati.'
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
