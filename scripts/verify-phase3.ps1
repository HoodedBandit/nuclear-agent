param(
    [switch]$SkipE2E,
    [switch]$SkipReleaseEval,
    [switch]$SkipSoak,
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
$packageOutputRoot = if ([string]::IsNullOrWhiteSpace($PackageOutputRoot)) {
    Join-Path $repoRoot "target\phase3\package"
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

Push-Location $repoRoot
try {
    Invoke-Step "beta verification" {
        $verifyBetaParams = @{}
        if ($SkipE2E) {
            $verifyBetaParams["SkipE2E"] = $true
        }
        if ($SkipReleaseEval) {
            $verifyBetaParams["SkipReleaseEval"] = $true
        }
        & (Join-Path $PSScriptRoot "verify-beta.ps1") @verifyBetaParams
    }

    Invoke-Step "package canonical release bundle" {
        & (Join-Path $PSScriptRoot "package-release.ps1") -OutputRoot $packageOutputRoot -Clean
        if ($LASTEXITCODE -ne 0) {
            throw "package-release failed"
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
            throw "Phase 3 soak requires -Token or AGENT_TOKEN unless -SkipSoak is passed."
        }
        Invoke-Step "daemon soak harness" {
            & (Join-Path $PSScriptRoot "run-soak.ps1") `
                -Token $effectiveToken `
                -BaseUrl $BaseUrl `
                -Workspace $Workspace `
                -OutputRoot $soakOutputRoot
            if ($LASTEXITCODE -ne 0) {
                throw "run-soak failed"
            }
        }
        $latestSoakRun = Get-ChildItem -Path $soakOutputRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($latestSoakRun) {
            $candidate = Join-Path $latestSoakRun.FullName "summary.json"
            if (Test-Path $candidate) {
                $soakSummaryPath = $candidate
            }
        }
    }

    Invoke-Step "write release record" {
        $recordParams = @{
            PackageRoot = $packageOutputRoot
            OutputRoot  = $releaseRecordOutputRoot
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
