param(
    [switch]$SkipE2E,
    [switch]$SkipReleaseEval,
    [string]$TaskFile = ".\\benchmarks\\release-eval\\tasks.jsonl",
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
$benchmarkOutputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    Join-Path $repoRoot "target\verify-beta\release-eval"
} else {
    $OutputRoot
}

Push-Location $repoRoot
try {
    Invoke-Step "baseline workspace verification" {
        & (Join-Path $PSScriptRoot "verify-workspace.ps1")
    }

    Invoke-Step "phase 1 isolated runtime smoke" {
        & (Join-Path $PSScriptRoot "verify-phase1.ps1") -BinaryPath $releaseBinary
    }

    Invoke-Step "phase 2 operator surface smoke" {
        & (Join-Path $PSScriptRoot "verify-phase2.ps1") -BinaryPath $releaseBinary
    }

    Invoke-Step "cargo clippy --workspace --all-targets --all-features -- -D warnings" {
        cargo clippy --workspace --all-targets --all-features --target-dir (Join-Path $repoRoot "target\verify-workspace") -- -D warnings
    }

    if (-not $SkipE2E) {
        Invoke-Step "dashboard Playwright E2E" {
            npm.cmd run test:e2e
        }
    }

    if (-not $SkipReleaseEval) {
        Invoke-Step "release-eval benchmark suite" {
            if (-not (Test-Path $releaseBinary)) {
                throw "Release benchmark suite requires a built CLI at $releaseBinary"
            }
            & (Join-Path $PSScriptRoot "run-bench.ps1") `
                -TaskFile $TaskFile `
                -BinaryPath $releaseBinary `
                -OutputRoot $benchmarkOutputRoot
            if ($LASTEXITCODE -ne 0) {
                throw "Release benchmark suite failed"
            }
        }
    }
} finally {
    Pop-Location
}
