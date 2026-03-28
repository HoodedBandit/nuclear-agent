$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    Write-Host "`n==> $Label" -ForegroundColor Cyan
    & $Action
    if (-not $?) {
        throw "Step failed: $Label"
    }
}

function Assert-Exists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    if (-not (Test-Path $Path)) {
        throw "$Label was not found at $Path"
    }
}

function Get-Version {
    param([string]$BinaryPath)

    $output = & $BinaryPath --version 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Version check failed for $BinaryPath.`n$output"
    }

    return $output.Trim()
}

function Get-CargoTargetRoot {
    param([string]$RepoRoot)

    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
            return $env:CARGO_TARGET_DIR
        }
        return [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $env:CARGO_TARGET_DIR))
    }

    return (Join-Path $RepoRoot "target")
}

function Invoke-IsolatedInstall {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [Parameter(Mandatory = $true)]
        [string]$ScenarioRoot
    )

    $previousUserProfile = $env:USERPROFILE
    $previousLocalAppData = $env:LOCALAPPDATA
    $previousHome = $env:HOME

    try {
        $env:USERPROFILE = Join-Path $ScenarioRoot "UserProfile"
        $env:LOCALAPPDATA = Join-Path $ScenarioRoot "LocalAppData"
        $env:HOME = $env:USERPROFILE

        New-Item -ItemType Directory -Force -Path $env:USERPROFILE | Out-Null
        New-Item -ItemType Directory -Force -Path $env:LOCALAPPDATA | Out-Null

        & powershell -NoProfile -ExecutionPolicy Bypass `
            -File (Join-Path $RepoRoot "install.ps1") `
            -NoPathPersist `
            -PreferSourceBuild `
            -SkipPlaywrightSetup
        if ($LASTEXITCODE -ne 0) {
            throw "install.ps1 failed with exit code $LASTEXITCODE"
        }
    } finally {
        if ($null -eq $previousUserProfile) {
            Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue
        } else {
            $env:USERPROFILE = $previousUserProfile
        }
        if ($null -eq $previousLocalAppData) {
            Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue
        } else {
            $env:LOCALAPPDATA = $previousLocalAppData
        }
        if ($null -eq $previousHome) {
            Remove-Item Env:HOME -ErrorAction SilentlyContinue
        } else {
            $env:HOME = $previousHome
        }
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$tempRoot = Join-Path $repoRoot "target\install-smoke\windows"
$cargoTargetRoot = Get-CargoTargetRoot -RepoRoot $repoRoot
$releaseLegacyBinary = Join-Path $cargoTargetRoot "release\autism.exe"

Push-Location $repoRoot
try {
    Invoke-Step "prepare Windows installer smoke workspace" {
        Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    }

    Invoke-Step "build release compatibility binaries" {
        cargo build --release -p nuclear --bin nuclear --bin autism
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
        Assert-Exists -Path $releaseLegacyBinary -Label "legacy compatibility binary"
    }

    $freshScenarioRoot = Join-Path $tempRoot "fresh-default"
    Invoke-Step "fresh install uses the canonical Nuclear path" {
        Invoke-IsolatedInstall -RepoRoot $repoRoot -ScenarioRoot $freshScenarioRoot

        $installDir = Join-Path $freshScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $legacyDir = Join-Path $freshScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        $nuclearBinary = Join-Path $installDir "nuclear.exe"
        $legacyBinary = Join-Path $installDir "autism.exe"

        Assert-Exists -Path $nuclearBinary -Label "canonical binary"
        Assert-Exists -Path $legacyBinary -Label "legacy compatibility binary"
        if (Test-Path $legacyDir) {
            throw "Fresh installs should not create the legacy default directory at $legacyDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
        [void](Get-Version -BinaryPath $legacyBinary)
    }

    $upgradeScenarioRoot = Join-Path $tempRoot "upgrade-legacy-default"
    Invoke-Step "legacy default install upgrades in place" {
        $legacyInstallDir = Join-Path $upgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        New-Item -ItemType Directory -Force -Path $legacyInstallDir | Out-Null
        Copy-Item -Force $releaseLegacyBinary (Join-Path $legacyInstallDir "autism.exe")

        Invoke-IsolatedInstall -RepoRoot $repoRoot -ScenarioRoot $upgradeScenarioRoot

        $canonicalDir = Join-Path $upgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $nuclearBinary = Join-Path $legacyInstallDir "nuclear.exe"
        $legacyBinary = Join-Path $legacyInstallDir "autism.exe"

        Assert-Exists -Path $nuclearBinary -Label "upgraded canonical binary"
        Assert-Exists -Path $legacyBinary -Label "upgraded legacy compatibility binary"
        if (Test-Path $canonicalDir) {
            throw "Legacy upgrades should remain in the existing install root instead of creating $canonicalDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
        [void](Get-Version -BinaryPath $legacyBinary)
    }
} finally {
    Pop-Location
}
