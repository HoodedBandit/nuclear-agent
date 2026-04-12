$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "common.ps1")

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

function Assert-DoesNotExist {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    if (Test-Path $Path) {
        throw "$Label should not exist at $Path"
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
Push-Location $repoRoot
try {
    Invoke-Step "prepare Windows installer smoke workspace" {
        Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    }

    Invoke-Step "build release binaries" {
        cargo build --release -p nuclear --bin nuclear
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
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

        Assert-Exists -Path $nuclearBinary -Label "canonical binary"
        if (Test-Path $legacyDir) {
            throw "Fresh installs should not create the legacy default directory at $legacyDir"
        }
        if (Test-Path (Join-Path $installDir "autism.exe")) {
            throw "Fresh installs must not leave a legacy launcher in $installDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
    }

    $upgradeScenarioRoot = Join-Path $tempRoot "upgrade-legacy-default"
    Invoke-Step "legacy default install migrates to the canonical root" {
        $legacyInstallDir = Join-Path $upgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        New-Item -ItemType Directory -Force -Path $legacyInstallDir | Out-Null
        Set-Content -Path (Join-Path $legacyInstallDir "autism.exe") -Encoding Ascii -Value "legacy"

        Invoke-IsolatedInstall `
            -InstallerRoot $repoRoot `
            -ScenarioRoot $upgradeScenarioRoot `
            -PreferSourceBuild `
            -SkipPlaywrightSetup

        $canonicalDir = Join-Path $upgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $nuclearBinary = Join-Path $canonicalDir "nuclear.exe"

        Assert-Exists -Path $nuclearBinary -Label "upgraded canonical binary"
        if (Test-Path (Join-Path $canonicalDir "autism.exe")) {
            throw "Legacy upgrades must remove the old launcher from the canonical install root"
        }
        if (Test-Path $legacyInstallDir) {
            throw "Legacy upgrades should migrate out of the old install root at $legacyInstallDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
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

        Assert-Exists -Path $nuclearBinary -Label "packaged canonical binary"
        if (Test-Path $legacyDir) {
            throw "Packaged fresh installs should not create the legacy default directory at $legacyDir"
        }
        if (Test-Path (Join-Path $installDir "autism.exe")) {
            throw "Packaged fresh installs must not leave a legacy launcher in $installDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
    }

    $packagedUpgradeScenarioRoot = Join-Path $tempRoot "packaged-upgrade-legacy-default"
    Invoke-Step "packaged bundle legacy install migrates to the canonical root" {
        $legacyInstallDir = Join-Path $packagedUpgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Autism\bin"
        New-Item -ItemType Directory -Force -Path $legacyInstallDir | Out-Null
        Set-Content -Path (Join-Path $legacyInstallDir "autism.exe") -Encoding Ascii -Value "legacy"

        Invoke-IsolatedInstall `
            -InstallerRoot $packageDir.FullName `
            -ScenarioRoot $packagedUpgradeScenarioRoot `
            -SkipPlaywrightSetup

        $canonicalDir = Join-Path $packagedUpgradeScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $nuclearBinary = Join-Path $canonicalDir "nuclear.exe"

        Assert-Exists -Path $nuclearBinary -Label "packaged upgraded canonical binary"
        if (Test-Path (Join-Path $canonicalDir "autism.exe")) {
            throw "Packaged legacy upgrades must remove the old launcher from the canonical install root"
        }
        if (Test-Path $legacyInstallDir) {
            throw "Packaged legacy upgrades should migrate out of the old install root at $legacyInstallDir"
        }

        [void](Get-Version -BinaryPath $nuclearBinary)
    }

    $rollbackScenarioRoot = Join-Path $tempRoot "packaged-rollback"
    Invoke-Step "packaged install writes rollback state and restores the previous managed binary" {
        Invoke-IsolatedInstall `
            -InstallerRoot $packageDir.FullName `
            -ScenarioRoot $rollbackScenarioRoot `
            -SkipPlaywrightSetup

        $installDir = Join-Path $rollbackScenarioRoot "LocalAppData\Programs\NuclearAI\Nuclear\bin"
        $nuclearBinary = Join-Path $installDir "nuclear.exe"
        $rollbackScript = Join-Path $installDir "nuclear-rollback.ps1"
        $rollbackWrapper = Join-Path $installDir "nuclear-rollback.cmd"
        $installStatePath = Join-Path $installDir "install-state.json"

        Assert-Exists -Path $nuclearBinary -Label "rollback scenario canonical binary"
        Assert-Exists -Path $rollbackScript -Label "rollback script"
        Assert-Exists -Path $rollbackWrapper -Label "rollback wrapper"
        Assert-Exists -Path $installStatePath -Label "install state"

        $baselineHash = (Get-FileHash -Path $nuclearBinary -Algorithm SHA256).Hash.ToLowerInvariant()

        Invoke-IsolatedInstall `
            -InstallerRoot $packageDir.FullName `
            -ScenarioRoot $rollbackScenarioRoot `
            -SkipPlaywrightSetup

        $installState = Get-Content -Path $installStatePath -Raw | ConvertFrom-Json
        $rollbackBinary = [string]$installState.rollback_binary
        Assert-Exists -Path $rollbackBinary -Label "rollback backup binary"

        Set-Content -Path $nuclearBinary -Encoding Ascii -Value "broken"
        & powershell `
            -NoProfile `
            -ExecutionPolicy Bypass `
            -File $rollbackScript `
            -InstallDir $installDir
        if ($LASTEXITCODE -ne 0) {
            throw "rollback script failed"
        }

        $restoredVersion = Get-Version -BinaryPath $nuclearBinary
        if ([string]::IsNullOrWhiteSpace($restoredVersion)) {
            throw "restored binary did not return a version string"
        }

        $restoredHash = (Get-FileHash -Path $nuclearBinary -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($restoredHash -ne $baselineHash) {
            throw "rollback did not restore the previously installed binary"
        }

        Assert-DoesNotExist -Path (Join-Path $installDir "autism.exe") -Label "legacy launcher after rollback"
    }
} finally {
    Pop-Location
}
