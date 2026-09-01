[CmdletBinding()]
param(
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$runtimeDirectory = Join-Path $repoRoot '.runtime'
$runtimeFile = Join-Path $runtimeDirectory 'runtime.json'
$urlFile = Join-Path $runtimeDirectory 'current-url.txt'

function Stop-TrackedProcess {
    param(
        [Parameter(Mandatory)] $Metadata,
        [Parameter(Mandatory)] [string] $Label
    )

    $process = Get-Process -Id ([int]$Metadata.pid) -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        if (-not $Quiet) {
            Write-Host "$Label is already stopped."
        }
        return
    }

    $expectedPath = [System.IO.Path]::GetFullPath([string]$Metadata.path)
    $actualPath = $null
    try {
        $actualPath = [System.IO.Path]::GetFullPath($process.Path)
    }
    catch {
        throw "Could not verify $Label process $($process.Id); refusing to stop an unverified PID."
    }
    if (-not $actualPath.Equals($expectedPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "PID $($process.Id) no longer belongs to $Label; refusing to stop it."
    }

    $expectedStart = ([DateTime]$Metadata.started_at).ToUniversalTime()
    $actualStart = $process.StartTime.ToUniversalTime()
    if ([Math]::Abs(($actualStart - $expectedStart).TotalSeconds) -gt 2) {
        throw "PID $($process.Id) start time does not match $Label metadata; refusing to stop it."
    }

    Stop-Process -Id $process.Id
    try {
        Wait-Process -Id $process.Id -Timeout 5 -ErrorAction Stop
    }
    catch {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    if (-not $Quiet) {
        Write-Host "Stopped $Label (PID $($process.Id))."
    }
}

if (-not (Test-Path -LiteralPath $runtimeFile -PathType Leaf)) {
    if (-not $Quiet) {
        Write-Host 'No tracked VibePing PoC session is running.'
    }
    return
}

$metadata = Get-Content -LiteralPath $runtimeFile -Raw | ConvertFrom-Json
Stop-TrackedProcess -Metadata $metadata.tunnel -Label 'Cloudflare Quick Tunnel'
Stop-TrackedProcess -Metadata $metadata.server -Label 'Rust server'

foreach ($path in @($runtimeFile, $urlFile)) {
    if (Test-Path -LiteralPath $path) {
        Remove-Item -LiteralPath $path -Force
    }
}

if (-not $Quiet) {
    Write-Host 'VibePing PoC stopped cleanly.'
}
