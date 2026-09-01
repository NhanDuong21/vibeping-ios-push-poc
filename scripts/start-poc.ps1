[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()

$repoRoot = Split-Path -Parent $PSScriptRoot
$runtimeDirectory = Join-Path $repoRoot '.runtime'
$runtimeFile = Join-Path $runtimeDirectory 'runtime.json'
$urlFile = Join-Path $runtimeDirectory 'current-url.txt'
$serverOut = Join-Path $runtimeDirectory 'server.stdout.log'
$serverErr = Join-Path $runtimeDirectory 'server.stderr.log'
$tunnelOut = Join-Path $runtimeDirectory 'cloudflared.stdout.log'
$tunnelErr = Join-Path $runtimeDirectory 'cloudflared.stderr.log'
$binary = Join-Path $repoRoot 'target\release\vibeping-push-poc.exe'
$healthUrl = 'http://127.0.0.1:8787/api/health'

function Resolve-Tool {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [string[]] $FallbackPaths = @()
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }
    foreach ($candidate in $FallbackPaths) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }
    throw "$Name was not found. See README.md prerequisites."
}

function Wait-ForHealth {
    param([System.Diagnostics.Process] $Process)

    $deadline = [DateTime]::UtcNow.AddSeconds(35)
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($Process.HasExited) {
            throw "The Rust server exited before becoming healthy. See $serverErr"
        }
        try {
            $response = Invoke-RestMethod -Uri $healthUrl -TimeoutSec 2
            if ($response.status -eq 'ok') {
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 400
        }
    }
    throw "The Rust server did not become healthy within 35 seconds. See $serverErr"
}

function Wait-ForTunnelUrl {
    param([System.Diagnostics.Process] $Process)

    $deadline = [DateTime]::UtcNow.AddSeconds(75)
    $pattern = 'https://[a-z0-9-]+\.trycloudflare\.com'
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($Process.HasExited) {
            throw "cloudflared exited before creating a Quick Tunnel. See $tunnelErr"
        }
        $content = @()
        foreach ($path in @($tunnelOut, $tunnelErr)) {
            if (Test-Path -LiteralPath $path) {
                $content += Get-Content -LiteralPath $path -Raw -ErrorAction SilentlyContinue
            }
        }
        $match = [regex]::Match(($content -join "`n"), $pattern)
        if ($match.Success) {
            return $match.Value
        }
        Start-Sleep -Milliseconds 500
    }
    throw "No trycloudflare.com URL appeared within 75 seconds. See $tunnelErr"
}

New-Item -ItemType Directory -Force -Path $runtimeDirectory | Out-Null
if (Test-Path -LiteralPath $runtimeFile) {
    & (Join-Path $PSScriptRoot 'stop-poc.ps1') -Quiet
}
foreach ($logFile in @($serverOut, $serverErr, $tunnelOut, $tunnelErr, $urlFile)) {
    if (Test-Path -LiteralPath $logFile) {
        Remove-Item -LiteralPath $logFile -Force
    }
}

$cargo = Resolve-Tool -Name 'cargo' -FallbackPaths @((Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'))
$cloudflared = Resolve-Tool -Name 'cloudflared' -FallbackPaths @(
    (Join-Path $env:ProgramFiles 'cloudflared\cloudflared.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'cloudflared\cloudflared.exe')
)

$serverProcess = $null
$tunnelProcess = $null
try {
    Push-Location $repoRoot
    try {
        if (-not $SkipBuild) {
            Write-Host 'Building the Rust release binary...'
            & $cargo build --release
            if ($LASTEXITCODE -ne 0) {
                throw "cargo build --release failed with exit code $LASTEXITCODE"
            }
        }
    }
    finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Release binary not found at $binary"
    }

    $serverProcess = Start-Process -FilePath $binary -ArgumentList @('serve') `
        -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput $serverOut -RedirectStandardError $serverErr
    Wait-ForHealth -Process $serverProcess

    $tunnelProcess = Start-Process -FilePath $cloudflared `
        -ArgumentList @('tunnel', '--url', 'http://127.0.0.1:8787', '--no-autoupdate') `
        -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput $tunnelOut -RedirectStandardError $tunnelErr
    $publicUrl = Wait-ForTunnelUrl -Process $tunnelProcess

    $metadata = [ordered]@{
        started_at = [DateTime]::UtcNow.ToString('o')
        local_url = 'http://127.0.0.1:8787'
        public_url = $publicUrl
        server = [ordered]@{
            pid = $serverProcess.Id
            path = $binary
            started_at = $serverProcess.StartTime.ToUniversalTime().ToString('o')
        }
        tunnel = [ordered]@{
            pid = $tunnelProcess.Id
            path = $cloudflared
            started_at = $tunnelProcess.StartTime.ToUniversalTime().ToString('o')
        }
    }
    $metadata | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $runtimeFile -Encoding UTF8
    Set-Content -LiteralPath $urlFile -Value $publicUrl -Encoding UTF8

    Write-Host ''
    Write-Host 'VibePing iOS Push PoC'
    Write-Host '────────────────────────────────────'
    Write-Host ''
    Write-Host 'Local server'
    Write-Host '✓ http://127.0.0.1:8787' -ForegroundColor Green
    Write-Host ''
    Write-Host 'Cloudflare Quick Tunnel'
    Write-Host "✓ $publicUrl" -ForegroundColor Green
    Write-Host ''
    Write-Host 'iPhone:'
    Write-Host '1. Open the HTTPS URL in Safari'
    Write-Host '2. Share → Add to Home Screen'
    Write-Host '3. Close Safari and launch VibePing from the Home Screen'
    Write-Host '4. Tap Enable notifications, then Allow'
    Write-Host ''
    Write-Host 'After the app shows Subscription Active:'
    Write-Host '.\target\release\vibeping-push-poc.exe send'
}
catch {
    foreach ($process in @($tunnelProcess, $serverProcess)) {
        if ($null -ne $process -and -not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    }
    throw
}
