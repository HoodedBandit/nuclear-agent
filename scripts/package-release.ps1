param(
    [string]$OutputRoot = "",
    [switch]$Clean,
    [switch]$RequireSigning
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Resolve-PythonCommand {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "python"
            Arguments  = @()
        }
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "py"
            Arguments  = @("-3")
        }
    }
    throw "Python is required to generate release metadata."
}

function Get-WorkspaceVersion {
    param([string]$CargoTomlPath)

    $raw = Get-Content -Path $CargoTomlPath -Raw
    $match = [regex]::Match($raw, '(?ms)^\[workspace\.package\]\s*(?<body>.*?)(^\[|\z)')
    if (-not $match.Success) {
        throw "Could not locate [workspace.package] in $CargoTomlPath"
    }

    $versionMatch = [regex]::Match($match.Groups["body"].Value, '(?m)^\s*version\s*=\s*"(?<version>[^"]+)"')
    if (-not $versionMatch.Success) {
        throw "Could not determine workspace version from $CargoTomlPath"
    }

    return $versionMatch.Groups["version"].Value
}

function Get-ArchTag {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($arch.ToUpperInvariant()) {
        "ARM64" { return "arm64" }
        default { return "x64" }
    }
}

function Get-SourceSnapshotItems {
    return @(
        ".cargo",
        ".github",
        "benchmarks",
        "crates",
        "docs",
        "harness",
        "scripts",
        "tests",
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "LICENSE",
        "deny.toml",
        "install",
        "install.cmd",
        "install.ps1",
        "package-lock.json",
        "package.json",
        "playwright.config.cjs",
        "README.md",
        "PACKAGE_README.md",
        "ui/dashboard/eslint.config.js",
        "ui/dashboard/index.html",
        "ui/dashboard/package-lock.json",
        "ui/dashboard/package.json",
        "ui/dashboard/src",
        "ui/dashboard/tsconfig.json",
        "ui/dashboard/vite.config.ts"
    )
}

function Copy-SnapshotItem {
    param(
        [string]$RepoRoot,
        [string]$RelativePath,
        [string]$DestinationRoot
    )

    $sourcePath = Join-Path $RepoRoot $RelativePath
    if (-not (Test-Path $sourcePath)) {
        return
    }

    $destinationPath = Join-Path $DestinationRoot $RelativePath
    $destinationParent = Split-Path -Parent $destinationPath
    if (-not [string]::IsNullOrWhiteSpace($destinationParent) -and -not (Test-Path $destinationParent)) {
        New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
    }

    Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Recurse -Force
}

function Get-GitCommitSha {
    param([string]$RepoRoot)

    try {
        $sha = (& git -C $RepoRoot rev-parse HEAD 2>$null | Out-String).Trim()
        if (-not [string]::IsNullOrWhiteSpace($sha)) {
            return $sha
        }
    } catch {
    }

    return ""
}

function Get-GitTreeState {
    param([string]$RepoRoot)

    try {
        $status = (& git -C $RepoRoot status --short --untracked-files=all 2>$null | Out-String).Trim()
        if ([string]::IsNullOrWhiteSpace($status)) {
            return @{
                CommitDirty = $false
                DirtyPaths  = @()
            }
        }

        $paths = $status -split "`r?`n" |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object {
                if ($_.Length -gt 3) { $_.Substring(3).Trim() } else { $_.Trim() }
            }
        return @{
            CommitDirty = $true
            DirtyPaths  = $paths
        }
    } catch {
        return @{
            CommitDirty = $false
            DirtyPaths  = @()
        }
    }
}

function Get-ReleaseBinaryPath {
    param(
        [string]$RepoRoot,
        [string]$BinaryName
    )

    $cargoTargetRoot = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
            $env:CARGO_TARGET_DIR
        } else {
            [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $env:CARGO_TARGET_DIR))
        }
    } else {
        Join-Path $RepoRoot "target"
    }

    return Join-Path $cargoTargetRoot "release\$BinaryName"
}

function Ensure-ReleaseBinaries {
    param([string]$RepoRoot)

    $nuclearBinary = Get-ReleaseBinaryPath -RepoRoot $RepoRoot -BinaryName "nuclear.exe"

    if (Test-Path $nuclearBinary) {
        return @{
            Nuclear = $nuclearBinary
        }
    }

    Write-Step "Building release binary"
    Push-Location $RepoRoot
    try {
        & cargo build --release -p nuclear --bin nuclear
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path $nuclearBinary)) {
        throw "Release build completed but the expected nuclear binary was not found."
    }

    return @{
        Nuclear = $nuclearBinary
    }
}

function Write-Json {
    param(
        [string]$Path,
        $Payload
    )

    $Payload | ConvertTo-Json -Depth 8 | Set-Content -Path $Path -Encoding UTF8
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$version = Get-WorkspaceVersion -CargoTomlPath (Join-Path $repoRoot "Cargo.toml")
$archTag = Get-ArchTag
$platformTag = "windows-$archTag"
$bundleName = "nuclear-$version-$platformTag-full"
$outputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    Join-Path $repoRoot "dist"
} elseif ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    $OutputRoot
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
$bundleDir = Join-Path $outputRoot $bundleName
$archivePath = Join-Path $outputRoot "$bundleName.zip"
$archiveHashPath = Join-Path $outputRoot "$bundleName.zip.sha256.txt"
$sbomPath = Join-Path $outputRoot "$bundleName.sbom.spdx.json"
$provenancePath = Join-Path $outputRoot "$bundleName.provenance.json"
$signingStatusPath = Join-Path $outputRoot "$bundleName.signing.json"
$manifestPath = Join-Path $outputRoot "$bundleName.manifest.json"
$commitSha = Get-GitCommitSha -RepoRoot $repoRoot
$gitTreeState = Get-GitTreeState -RepoRoot $repoRoot
$releaseBinaries = Ensure-ReleaseBinaries -RepoRoot $repoRoot
$pythonCommand = Resolve-PythonCommand

if ($Clean) {
    Remove-Item -Recurse -Force $bundleDir -ErrorAction SilentlyContinue
    Remove-Item -Force $archivePath -ErrorAction SilentlyContinue
    Remove-Item -Force $archiveHashPath -ErrorAction SilentlyContinue
    Remove-Item -Force $sbomPath -ErrorAction SilentlyContinue
    Remove-Item -Force $provenancePath -ErrorAction SilentlyContinue
    Remove-Item -Force $signingStatusPath -ErrorAction SilentlyContinue
    Remove-Item -Force $manifestPath -ErrorAction SilentlyContinue
}

New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $bundleDir "bin\$platformTag") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $bundleDir "source") | Out-Null

Write-Step "Copying packaged installer surface"
Copy-Item -LiteralPath (Join-Path $repoRoot "install") -Destination (Join-Path $bundleDir "install") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "install.cmd") -Destination (Join-Path $bundleDir "install.cmd") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "install.ps1") -Destination (Join-Path $bundleDir "install.ps1") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "PACKAGE_README.md") -Destination (Join-Path $bundleDir "README.md") -Force

Write-Step "Copying bundled release binaries"
Copy-Item -LiteralPath $releaseBinaries.Nuclear -Destination (Join-Path $bundleDir "bin\$platformTag\nuclear.exe") -Force

Write-Step "Copying source snapshot"
foreach ($item in Get-SourceSnapshotItems) {
    Copy-SnapshotItem -RepoRoot $repoRoot -RelativePath $item -DestinationRoot (Join-Path $bundleDir "source")
}

$internalManifest = @{
    name       = $bundleName
    version    = $version
    platform   = $platformTag
    created_at = (Get-Date).ToUniversalTime().ToString("o")
    commit_sha = $commitSha
    commit_dirty = $gitTreeState.CommitDirty
    dirty_paths = $gitTreeState.DirtyPaths
    binaries   = @{
        canonical = @{
            name   = "nuclear.exe"
            sha256 = (Get-FileHash -Path (Join-Path $bundleDir "bin\$platformTag\nuclear.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    install = @{
        canonical_command = "nuclear"
        fresh_root        = "%LOCALAPPDATA%\\Programs\\NuclearAI\\Nuclear\\bin"
    }
}
Write-Json -Path (Join-Path $bundleDir "release-manifest.json") -Payload $internalManifest

Write-Step "Compressing packaged bundle"
if (Test-Path $archivePath) {
    Remove-Item -Force $archivePath
}
Compress-Archive -Path $bundleDir -DestinationPath $archivePath -Force

$archiveHash = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Path $archiveHashPath -Encoding Ascii -Value "$archiveHash  $([System.IO.Path]::GetFileName($archivePath))"

Write-Step "Generating SBOM"
& $pythonCommand.Executable @($pythonCommand.Arguments) `
    (Join-Path $PSScriptRoot "generate_sbom.py") `
    --repo-root $repoRoot `
    --bundle-name $bundleName `
    --version $version `
    --platform $platformTag `
    --output-path $sbomPath
if ($LASTEXITCODE -ne 0) {
    throw "generate_sbom.py failed"
}

$manifest = @{
    name              = $bundleName
    version           = $version
    platform          = $platformTag
    created_at        = (Get-Date).ToUniversalTime().ToString("o")
    commit_sha        = $commitSha
    commit_dirty      = $gitTreeState.CommitDirty
    dirty_paths       = $gitTreeState.DirtyPaths
    bundle_dir        = $bundleDir
    archive_path      = $archivePath
    archive_sha256    = $archiveHash
    checksum_path     = $archiveHashPath
    package_readme    = (Join-Path $bundleDir "README.md")
    internal_manifest = (Join-Path $bundleDir "release-manifest.json")
    sbom_path         = $sbomPath
    provenance_path   = $provenancePath
    signing_status    = $signingStatusPath
    signing_required  = $RequireSigning.IsPresent
    signing_hook      = $env:NUCLEAR_SIGNING_HOOK
}
Write-Json -Path $manifestPath -Payload $manifest

Write-Step "Generating provenance"
& $pythonCommand.Executable @($pythonCommand.Arguments) `
    (Join-Path $PSScriptRoot "generate_provenance.py") `
    --manifest-path $manifestPath `
    --archive-path $archivePath `
    --checksum-path $archiveHashPath `
    --sbom-path $sbomPath `
    --output-path $provenancePath
if ($LASTEXITCODE -ne 0) {
    throw "generate_provenance.py failed"
}

Write-Step "Collecting signatures"
& $pythonCommand.Executable @($pythonCommand.Arguments) `
    (Join-Path $PSScriptRoot "sign_artifacts.py") `
    --manifest-path $manifestPath `
    --artifacts $archivePath $archiveHashPath $manifestPath $sbomPath $provenancePath `
    --status-path $signingStatusPath
if ($LASTEXITCODE -ne 0) {
    throw "sign_artifacts.py failed"
}

if ($RequireSigning) {
    $signingStatus = Get-Content -Path $signingStatusPath -Raw | ConvertFrom-Json
    if (-not $signingStatus.enabled) {
        throw "Signing is required but NUCLEAR_SIGNING_HOOK was not configured."
    }
    if (-not $signingStatus.signatures.PSObject.Properties.Name.Count) {
        throw "Signing is required but no artifact signatures were recorded."
    }
}

Write-Host "Package output written to $bundleDir"
Write-Host "Archive written to $archivePath"
Write-Host "Manifest written to $manifestPath"
