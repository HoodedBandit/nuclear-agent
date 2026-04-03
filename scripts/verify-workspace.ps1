$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    $maxAttempts = if ($Label -like "cargo *") { 3 } else { 1 }
    $lastFailure = $null

    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        Write-Host "`n==> $Label" -ForegroundColor Cyan
        $global:LASTEXITCODE = 0
        $failed = $false
        $failureMessage = $null

        try {
            & $Action
            if (-not $?) {
                $failed = $true
                $failureMessage = "Step failed: $Label"
            } elseif ($LASTEXITCODE -ne 0) {
                $failed = $true
                $failureMessage = "Step failed: $Label (exit code $LASTEXITCODE)"
            }
        } catch {
            $failed = $true
            $failureMessage = $_.Exception.Message
        }

        if (-not $failed) {
            return
        }

        $lastFailure = $failureMessage
        if ($attempt -lt $maxAttempts) {
            Write-Warning "$failureMessage; retrying ($attempt/$maxAttempts)"
            Start-Sleep -Milliseconds 1500
        }
    }

    throw $lastFailure
}

function Invoke-RequiredCargoTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Tool,
        [AllowEmptyCollection()]
        [string[]]$Args = @()
    )

    $toolCommand = Get-Command ("cargo-" + $Tool) -ErrorAction SilentlyContinue
    if ($null -eq $toolCommand) {
        throw "cargo-$Tool is required. Install with: cargo install cargo-$Tool --locked"
    }

    & cargo $Tool @Args
}

function Invoke-RuntimeSmokeValidation {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [Parameter(Mandatory = $true)]
        [string]$CargoTargetDir
    )

    $binaryPath = Join-Path $CargoTargetDir "release\nuclear.exe"
    if (-not (Test-Path $binaryPath)) {
        throw "Runtime smoke requires a built CLI at $binaryPath"
    }

    $outputRoot = Join-Path $CargoTargetDir "runtime-cert-smoke"
    Remove-Item -Recurse -Force $outputRoot -ErrorAction SilentlyContinue
    & (Join-Path $RepoRoot "scripts\run-harness.ps1") `
        -Lane "runtime-cert" `
        -BinaryPath $binaryPath `
        -OutputRoot $outputRoot `
        -TaskFilter "install-smoke,support-bundle-smoke"
    if ($LASTEXITCODE -ne 0) {
        throw "Runtime smoke run failed"
    }

    $runDir = Get-ChildItem -Path $outputRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($null -eq $runDir) {
        throw "Runtime smoke did not produce an output directory"
    }

    $laneDir = Join-Path $runDir.FullName "runtime-cert"
    $summaryPath = Join-Path $laneDir "summary.json"
    $summaryMarkdownPath = Join-Path $laneDir "summary.md"
    if (-not (Test-Path $summaryPath) -or -not (Test-Path $summaryMarkdownPath)) {
        throw "Runtime smoke did not produce summary artifacts"
    }

    $summary = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
    if ($summary.failed -ne 0 -or $summary.passed -lt 1) {
        throw "Runtime smoke summary indicates failure"
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = Join-Path $repoRoot "target/verify-workspace"
try {
    Invoke-Step "source LOC guard" { & (Join-Path $PSScriptRoot "check-max-loc.ps1") }
    Invoke-Step "cargo fmt --all --check" { cargo fmt --all --check }
    Invoke-Step "cargo check --workspace" { cargo check --workspace }
    Invoke-Step "cargo test --workspace" { cargo test --workspace }
    Invoke-Step "cargo build --release --bin nuclear" {
        cargo build --release --bin nuclear
    }
    Invoke-Step "runtime smoke validation" {
        Invoke-RuntimeSmokeValidation -RepoRoot $repoRoot -CargoTargetDir $env:CARGO_TARGET_DIR
    }
    Invoke-Step "cargo tree --workspace --duplicates" {
        cargo tree --workspace --duplicates
    }
    Invoke-Step "cargo audit" { Invoke-RequiredCargoTool -Tool "audit" }
    Invoke-Step "cargo deny check advisories licenses bans" {
        Invoke-RequiredCargoTool -Tool "deny" -Args @("check", "advisories", "licenses", "bans")
    }
} finally {
    if ($null -eq $previousCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
    Pop-Location
}
