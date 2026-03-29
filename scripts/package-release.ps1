param(
    [string]$OutputRoot = "",
    [switch]$Clean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "`n==> $Message" -ForegroundColor Cyan
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
        "benchmarks",
        "crates",
        "docs",
        "scripts",
        "tests",
        "Cargo.lock",
        "Cargo.toml",
        "deny.toml",
        "package-lock.json",
        "package.json",
        "playwright.config.cjs",
        "PROJECT_REVIEW.md",
        "README.md",
        "RECOVERY_REPORT.md",
        "WORKTREE_LOG_2026-03-13.md",
        "PACKAGE_README.md"
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
    $legacyBinary = Get-ReleaseBinaryPath -RepoRoot $RepoRoot -BinaryName "autism.exe"

    if ((Test-Path $nuclearBinary) -and (Test-Path $legacyBinary)) {
        return @{
            Nuclear = $nuclearBinary
            Legacy  = $legacyBinary
        }
    }

    Write-Step "Building release compatibility binaries"
    Push-Location $RepoRoot
    try {
        & cargo build --release -p nuclear --bin nuclear --bin autism
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
    } finally {
        Pop-Location
    }

    if (-not (Test-Path $nuclearBinary) -or -not (Test-Path $legacyBinary)) {
        throw "Release build completed but expected binaries were not found."
    }

    return @{
        Nuclear = $nuclearBinary
        Legacy  = $legacyBinary
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
$manifestPath = Join-Path $outputRoot "$bundleName.manifest.json"
$commitSha = Get-GitCommitSha -RepoRoot $repoRoot
$releaseBinaries = Ensure-ReleaseBinaries -RepoRoot $repoRoot

if ($Clean) {
    Remove-Item -Recurse -Force $bundleDir -ErrorAction SilentlyContinue
    Remove-Item -Force $archivePath -ErrorAction SilentlyContinue
    Remove-Item -Force $archiveHashPath -ErrorAction SilentlyContinue
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
Copy-Item -LiteralPath $releaseBinaries.Legacy -Destination (Join-Path $bundleDir "bin\$platformTag\autism.exe") -Force

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
    binaries   = @{
        canonical = @{
            name   = "nuclear.exe"
            sha256 = (Get-FileHash -Path (Join-Path $bundleDir "bin\$platformTag\nuclear.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        legacy    = @{
            name   = "autism.exe"
            sha256 = (Get-FileHash -Path (Join-Path $bundleDir "bin\$platformTag\autism.exe") -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    install = @{
        canonical_command = "nuclear"
        legacy_command    = "autism"
        fresh_root        = "%LOCALAPPDATA%\\Programs\\NuclearAI\\Nuclear\\bin"
        legacy_root       = "%LOCALAPPDATA%\\Programs\\NuclearAI\\Autism\\bin"
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

$manifest = @{
    name              = $bundleName
    version           = $version
    platform          = $platformTag
    created_at        = (Get-Date).ToUniversalTime().ToString("o")
    commit_sha        = $commitSha
    bundle_dir        = $bundleDir
    archive_path      = $archivePath
    archive_sha256    = $archiveHash
    checksum_path     = $archiveHashPath
    package_readme    = (Join-Path $bundleDir "README.md")
    internal_manifest = (Join-Path $bundleDir "release-manifest.json")
}
Write-Json -Path $manifestPath -Payload $manifest

Write-Host "Package output written to $bundleDir"
Write-Host "Archive written to $archivePath"
Write-Host "Manifest written to $manifestPath"
