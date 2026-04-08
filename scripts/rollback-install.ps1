param(
    [string]$InstallDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-InstallDir {
    param([string]$RequestedInstallDir)

    if (-not [string]::IsNullOrWhiteSpace($RequestedInstallDir)) {
        return (Resolve-Path $RequestedInstallDir).Path
    }

    if ($PSScriptRoot) {
        return $PSScriptRoot
    }

    return Split-Path -Parent $PSCommandPath
}

function Stop-ManagedProcess {
    param([string]$BinaryPath)

    if (-not (Test-Path $BinaryPath)) {
        return
    }

    try {
        & $BinaryPath daemon stop *> $null
    } catch {
    }

    Start-Sleep -Milliseconds 750
    Get-CimInstance Win32_Process -Filter "Name = 'nuclear.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and
            ([System.IO.Path]::GetFullPath($_.ExecutablePath) -ieq [System.IO.Path]::GetFullPath($BinaryPath))
        } |
        ForEach-Object {
            try {
                Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop
            } catch {
            }
        }
}

$resolvedInstallDir = Resolve-InstallDir -RequestedInstallDir $InstallDir
$statePath = Join-Path $resolvedInstallDir "install-state.json"
if (-not (Test-Path $statePath)) {
    throw "Rollback state was not found at $statePath"
}

$state = Get-Content -Path $statePath -Raw | ConvertFrom-Json
$binaryPath = Join-Path $resolvedInstallDir "nuclear.exe"
$rollbackBinary = [string]$state.rollback_binary
if ([string]::IsNullOrWhiteSpace($rollbackBinary) -or -not (Test-Path $rollbackBinary)) {
    throw "Rollback binary was not found at $rollbackBinary"
}

Stop-ManagedProcess -BinaryPath $binaryPath

$tempPath = "$binaryPath.rollback"
Copy-Item -Force -LiteralPath $rollbackBinary -Destination $tempPath
Move-Item -Force -LiteralPath $tempPath -Destination $binaryPath

$version = & $binaryPath --version 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "Rollback completed but the restored binary failed version check.`n$version"
}

Write-Host "Restored $(($version.Trim()))"
