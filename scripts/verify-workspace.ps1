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

function Invoke-OptionalCargoTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Tool,
        [AllowEmptyCollection()]
        [string[]]$Args = @()
    )

    $cargoList = cargo --list
    if ($cargoList -notmatch "^\s+$Tool\s") {
        Write-Warning "cargo-$Tool is not installed; skipping. Install with: cargo install cargo-$Tool --locked"
        return
    }

    & cargo $Tool @Args
}

function Invoke-BenchmarkSmokeValidation {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [Parameter(Mandatory = $true)]
        [string]$CargoTargetDir
    )

    $binaryPath = Join-Path $CargoTargetDir "release\nuclear.exe"
    if (-not (Test-Path $binaryPath)) {
        throw "Benchmark smoke requires a built CLI at $binaryPath"
    }

    $taskFile = Join-Path $CargoTargetDir "benchmark-smoke.jsonl"
    $outputRoot = Join-Path $CargoTargetDir "benchmarks-smoke"
    Remove-Item -Recurse -Force $outputRoot -ErrorAction SilentlyContinue
    Set-Content -Path $taskFile -Encoding Ascii -Value @(
        '{"id":"repo-inspect-json","description":"Inspect the repository without a model round-trip","category":"repo_inspection","tags":["smoke","verify"],"command":["repo","inspect","--json"]}'
    )

    & (Join-Path $RepoRoot "scripts\run-bench.ps1") -TaskFile $taskFile -BinaryPath $binaryPath -OutputRoot $outputRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Benchmark smoke run failed"
    }

    $runDir = Get-ChildItem -Path $outputRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($null -eq $runDir) {
        throw "Benchmark smoke did not produce an output directory"
    }

    $summaryPath = Join-Path $runDir.FullName "summary.json"
    $summaryMarkdownPath = Join-Path $runDir.FullName "summary.md"
    if (-not (Test-Path $summaryPath) -or -not (Test-Path $summaryMarkdownPath)) {
        throw "Benchmark smoke did not produce summary artifacts"
    }

    $summary = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
    if ($summary.failed -ne 0 -or $summary.passed -lt 1) {
        throw "Benchmark smoke summary indicates failure"
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = Join-Path $repoRoot "target/verify-workspace"
try {
    Invoke-Step "source LOC guard" { & (Join-Path $PSScriptRoot "check-max-loc.ps1") }
    Invoke-Step "cargo check --workspace" { cargo check --workspace }
    Invoke-Step "cargo test --workspace" { cargo test --workspace }
    Invoke-Step "cargo build --release --bin nuclear --bin autism" {
        cargo build --release --bin nuclear --bin autism
    }
    Invoke-Step "benchmark smoke artifact validation" {
        Invoke-BenchmarkSmokeValidation -RepoRoot $repoRoot -CargoTargetDir $env:CARGO_TARGET_DIR
    }
    Invoke-Step "cargo tree --workspace --duplicates" {
        cargo tree --workspace --duplicates
    }
    Invoke-Step "cargo audit" {
        Invoke-OptionalCargoTool -Tool "audit"
    }
    Invoke-Step "cargo deny check advisories licenses bans" {
        Invoke-OptionalCargoTool -Tool "deny" -Args @("check", "advisories", "licenses", "bans")
    }
    Invoke-Step "cargo outdated -R" {
        Invoke-OptionalCargoTool -Tool "outdated" -Args @("-R")
    }
} finally {
    if ($null -eq $previousCargoTargetDir) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
    Pop-Location
}
