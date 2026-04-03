param(
    [switch]$SkipE2E,
    [switch]$SkipDeterministicCoding,
    [switch]$SkipReleaseEval,
    [string]$TaskFile = ".\\harness\\tasks\\coding\\tasks.json",
    [string]$OutputRoot = ""
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
$runtimeCertOutputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    Join-Path $repoRoot "target\verify-ga\runtime-cert"
} else {
    Join-Path $OutputRoot "runtime-cert"
}
$codingOutputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    Join-Path $repoRoot "target\verify-ga\coding-deterministic"
} else {
    Join-Path $OutputRoot "coding-deterministic"
}

if ($SkipReleaseEval) {
    Write-Warning "-SkipReleaseEval is deprecated. Use -SkipDeterministicCoding."
    $SkipDeterministicCoding = $true
}

Push-Location $repoRoot
try {
    Invoke-Step "baseline workspace verification" {
        & (Join-Path $PSScriptRoot "verify-workspace.ps1")
    }

    Invoke-Step "runtime certification lane" {
        & (Join-Path $PSScriptRoot "run-harness.ps1") `
            -Lane "runtime-cert" `
            -BinaryPath $releaseBinary `
            -OutputRoot $runtimeCertOutputRoot
        if ($LASTEXITCODE -ne 0) {
            throw "runtime-cert lane failed"
        }
    }

    Invoke-Step "cargo clippy --workspace --all-targets --all-features -- -D warnings" {
        cargo clippy --workspace --all-targets --all-features --target-dir (Join-Path $repoRoot "target\verify-workspace") -- -D warnings
    }

    if (-not $SkipE2E) {
        Invoke-Step "dashboard Playwright E2E" {
            npm.cmd run test:e2e
        }
    }

    if (-not $SkipDeterministicCoding) {
        Invoke-Step "deterministic coding harness" {
            if (-not (Test-Path $releaseBinary)) {
                throw "Deterministic coding harness requires a built CLI at $releaseBinary"
            }
            & (Join-Path $PSScriptRoot "run-harness.ps1") `
                -Lane "coding-deterministic" `
                -TaskFile $TaskFile `
                -BinaryPath $releaseBinary `
                -OutputRoot $codingOutputRoot
            if ($LASTEXITCODE -ne 0) {
                throw "Deterministic coding harness failed"
            }
        }
    }
} finally {
    Pop-Location
}
