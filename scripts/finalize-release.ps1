param(
    [switch]$SkipE2E,
    [switch]$SkipDeterministicCoding,
    [switch]$SkipCodingReference,
    [switch]$SkipReleaseEval,
    [switch]$SkipSoak,
    [switch]$SkipSigning,
    [string]$TaskFile = ".\\harness\\tasks\\coding\\tasks.json",
    [string]$ReferenceProfile = "",
    [string]$Alias = "",
    [string]$ProviderId = "",
    [string]$Model = "",
    [string]$ProviderKind = "",
    [string]$ReferenceBaseUrl = "",
    [string]$ApiKeyEnv = "",
    [string]$Token = "",
    [string]$BaseUrl = "http://127.0.0.1:42690",
    [string]$Workspace = "",
    [string]$PackageOutputRoot = "",
    [string]$ReleaseRecordOutputRoot = "",
    [string]$SoakOutputRoot = ""
)

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

$repoRoot = Split-Path -Parent $PSScriptRoot
$releaseBinary = Join-Path $repoRoot "target\verify-workspace\release\nuclear.exe"
$packageOutputRoot = if ([string]::IsNullOrWhiteSpace($PackageOutputRoot)) {
    Join-Path $repoRoot "target\release\package"
} else {
    $PackageOutputRoot
}
$releaseRecordOutputRoot = if ([string]::IsNullOrWhiteSpace($ReleaseRecordOutputRoot)) {
    Join-Path $repoRoot "target\release-records"
} else {
    $ReleaseRecordOutputRoot
}
$soakOutputRoot = if ([string]::IsNullOrWhiteSpace($SoakOutputRoot)) {
    Join-Path $repoRoot "target\soak"
} else {
    $SoakOutputRoot
}
$codingReferenceOutputRoot = Join-Path $repoRoot "target\finalize-release\coding-reference"

if ($SkipReleaseEval) {
    Write-Warning "-SkipReleaseEval is deprecated. Use -SkipDeterministicCoding and/or -SkipCodingReference."
    $SkipDeterministicCoding = $true
    $SkipCodingReference = $true
}

Push-Location $repoRoot
try {
    Invoke-Step "GA verification" {
        $verifyParams = @{}
        if ($SkipE2E) {
            $verifyParams["SkipE2E"] = $true
        }
        if ($SkipDeterministicCoding) {
            $verifyParams["SkipDeterministicCoding"] = $true
        }
        $verifyParams["TaskFile"] = $TaskFile
        & (Join-Path $PSScriptRoot "verify-ga.ps1") @verifyParams
    }

    Invoke-Step "package canonical release bundle" {
        $packageParams = @{
            OutputRoot = $packageOutputRoot
            Clean      = $true
        }
        if (-not $SkipSigning) {
            $packageParams["RequireSigning"] = $true
        }
        & (Join-Path $PSScriptRoot "package-release.ps1") @packageParams
        if ($LASTEXITCODE -ne 0) {
            throw "package-release failed"
        }
    }

    $codingReferenceSummaryPath = ""
    if (-not $SkipCodingReference) {
        Invoke-Step "reference coding harness" {
            $referenceParams = @{
                Lane       = "coding-reference"
                BinaryPath = $releaseBinary
                OutputRoot = $codingReferenceOutputRoot
                TaskFile   = $TaskFile
            }
            if ($ReferenceProfile) { $referenceParams["Profile"] = $ReferenceProfile }
            if ($Alias) { $referenceParams["Alias"] = $Alias }
            if ($ProviderId) { $referenceParams["ProviderId"] = $ProviderId }
            if ($Model) { $referenceParams["Model"] = $Model }
            if ($ProviderKind) { $referenceParams["ProviderKind"] = $ProviderKind }
            if ($ReferenceBaseUrl) { $referenceParams["BaseUrl"] = $ReferenceBaseUrl }
            if ($ApiKeyEnv) { $referenceParams["ApiKeyEnv"] = $ApiKeyEnv }
            & (Join-Path $PSScriptRoot "run-harness.ps1") @referenceParams
            if ($LASTEXITCODE -ne 0) {
                throw "coding-reference harness failed"
            }
        }
        $latestCodingReferenceRun = Get-ChildItem -Path $codingReferenceOutputRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($latestCodingReferenceRun) {
            $candidate = Join-Path $latestCodingReferenceRun.FullName "coding-reference\\summary.json"
            if (Test-Path $candidate) {
                $codingReferenceSummaryPath = $candidate
            }
        }
    }

    $soakSummaryPath = ""
    if (-not $SkipSoak) {
        $effectiveToken = if ([string]::IsNullOrWhiteSpace($Token)) {
            $env:AGENT_TOKEN
        } else {
            $Token
        }
        if ([string]::IsNullOrWhiteSpace($effectiveToken)) {
            throw "Finalize release requires -Token or AGENT_TOKEN unless -SkipSoak is passed."
        }
        Invoke-Step "daemon soak harness" {
            & (Join-Path $PSScriptRoot "run-harness.ps1") `
                -Lane "soak" `
                -Token $effectiveToken `
                -SoakBaseUrl $BaseUrl `
                -Workspace $Workspace `
                -OutputRoot $soakOutputRoot
            if ($LASTEXITCODE -ne 0) {
                throw "soak lane failed"
            }
        }
        $latestSoakRun = Get-ChildItem -Path $soakOutputRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($latestSoakRun) {
            $candidate = Join-Path $latestSoakRun.FullName "soak\\summary.json"
            if (Test-Path $candidate) {
                $soakSummaryPath = $candidate
            }
        }
    }

    Invoke-Step "write production release record" {
        $recordParams = @{
            PackageRoot              = $packageOutputRoot
            RuntimeCertRoot          = Join-Path $repoRoot "target\verify-ga\runtime-cert"
            CodingDeterministicRoot  = Join-Path $repoRoot "target\verify-ga\coding-deterministic"
            CodingReferenceRoot      = $codingReferenceOutputRoot
            OutputRoot               = $releaseRecordOutputRoot
        }
        if (-not $SkipCodingReference) {
            $recordParams["RequireCodingReference"] = $true
        }
        if (-not [string]::IsNullOrWhiteSpace($codingReferenceSummaryPath)) {
            $recordParams["CodingReferenceSummary"] = $codingReferenceSummaryPath
        }
        if (-not [string]::IsNullOrWhiteSpace($soakSummaryPath)) {
            $recordParams["SoakSummary"] = $soakSummaryPath
        }
        & (Join-Path $PSScriptRoot "write-release-record.ps1") @recordParams
        if ($LASTEXITCODE -ne 0) {
            throw "write-release-record failed"
        }
    }
} finally {
    Pop-Location
}
