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
        [string]$InstallerRoot,
        [Parameter(Mandatory = $true)]
        [string]$ScenarioRoot,
        [switch]$PreferSourceBuild,
        [switch]$SkipPlaywrightSetup
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

        $installArgs = @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", (Join-Path $InstallerRoot "install.ps1"),
            "-NoPathPersist"
        )
        if ($PreferSourceBuild) {
            $installArgs += "-PreferSourceBuild"
        }
        if ($SkipPlaywrightSetup) {
            $installArgs += "-SkipPlaywrightSetup"
        }

        & powershell @installArgs
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
$packageOutputRoot = Join-Path $tempRoot "package-output"
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
        Invoke-IsolatedInstall `
            -InstallerRoot $repoRoot `
            -ScenarioRoot $freshScenarioRoot `
            -PreferSourceBuild `
            -SkipPlaywrightSetup

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

        Invoke-IsolatedInstall `
            -InstallerRoot $repoRoot `
            -ScenarioRoot $upgradeScenarioRoot `
            -PreferSourceBuild `
            -SkipPlaywrightSetup

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

    Invoke-Step "package canonical Windows release bundle" {
        & (Join-Path $PSScriptRoot "package-release.ps1") -OutputRoot $packageOutputRoot -Clean
        if ($LASTEXITCODE -ne 0) {
            throw "package-release.ps1 failed"
        }
    }

    $packageDir = Get-ChildItem -Path $packageOutputRoot -Directory |
        Where-Object { $_.Name -like "nuclear-*-windows-*-full" } |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($null -eq $packageDir) {
        throw "Could not locate the packaged Windows bundle under $packageOutputRoot"
    }

    $packagedFreshScenarioRoot = Join-Path $tempRoot "packaged-fresh-default"
    Invoke-Step "packaged bundle fresh install uses the bundled nuclear binary" {
        Invoke-IsolatedInstall `
            -InstallerRoot $packageDir.FullName `
            -ScenarioRoot $packagedFreshScenarioRoot `
            -SkipPlaywrightSetup

        $installDir = Join-Path $packagedFreshScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $legacyDir = Join-Path $packagedFreshScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        $nuclearBinary = Join-Path $installDir "nuclear.exe"
        $legacyBinary = Join-Path $installDir "autism.exe"

        Assert-Exists -Path $nuclearBinary -Label "packaged canonical binary"
        Assert-Exists -Path $legacyBinary -Label "packaged legacy compatibility binary"
        if (Test-Path $legacyDir) {
            throw "Packaged fresh installs should not create the legacy default directory at $legacyDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
        [void](Get-Version -BinaryPath $legacyBinary)
    }

    $packagedUpgradeScenarioRoot = Join-Path $tempRoot "packaged-upgrade-legacy-default"
    Invoke-Step "packaged bundle legacy install upgrades in place" {
        $legacyInstallDir = Join-Path $packagedUpgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        New-Item -ItemType Directory -Force -Path $legacyInstallDir | Out-Null
        Copy-Item -Force $releaseLegacyBinary (Join-Path $legacyInstallDir "autism.exe")

        Invoke-IsolatedInstall `
            -InstallerRoot $packageDir.FullName `
            -ScenarioRoot $packagedUpgradeScenarioRoot `
            -SkipPlaywrightSetup

        $canonicalDir = Join-Path $packagedUpgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $nuclearBinary = Join-Path $legacyInstallDir "nuclear.exe"
        $legacyBinary = Join-Path $legacyInstallDir "autism.exe"

        Assert-Exists -Path $nuclearBinary -Label "packaged upgraded canonical binary"
        Assert-Exists -Path $legacyBinary -Label "packaged upgraded legacy compatibility binary"
        if (Test-Path $canonicalDir) {
            throw "Packaged legacy upgrades should remain in the existing install root instead of creating $canonicalDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
        [void](Get-Version -BinaryPath $legacyBinary)
    }
} finally {
    Pop-Location
}
