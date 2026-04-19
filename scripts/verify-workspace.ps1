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

    $cargoHome = if ([string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
        Join-Path $HOME ".cargo"
    } else {
        $env:CARGO_HOME
    }
    $toolPath = Join-Path $cargoHome "bin\cargo-$Tool.exe"
    $toolCommand = Get-Command "cargo-$Tool" -ErrorAction SilentlyContinue
    if (-not (Test-Path $toolPath) -and $null -eq $toolCommand) {
        $message = "cargo-$Tool is not installed. Install with: cargo install cargo-$Tool --locked"
        if ($env:CI -eq "true") {
            throw "$message Required in CI."
        }
        Write-Warning "$message Skipping local check."
        return
    }

    & cargo $Tool @Args
}

function Invoke-WorkspaceDependencyDriftCheck {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    if (Get-Command python -ErrorAction SilentlyContinue) {
        & python (Join-Path $RepoRoot "scripts\check-workspace-dependency-drift.py")
        return
    }

    if (Get-Command py -ErrorAction SilentlyContinue) {
        & py -3 (Join-Path $RepoRoot "scripts\check-workspace-dependency-drift.py")
        return
    }

    throw "Python is required for the workspace dependency drift check"
}

function Invoke-ReleaseGateScriptTests {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    if (Get-Command python -ErrorAction SilentlyContinue) {
        & python -m unittest discover -s (Join-Path $RepoRoot "scripts\tests") -p "test_*.py"
        return
    }

    if (Get-Command py -ErrorAction SilentlyContinue) {
        & py -3 -m unittest discover -s (Join-Path $RepoRoot "scripts\tests") -p "test_*.py"
        return
    }

    throw "Python is required for the release gate script tests"
}

function Resolve-NpmCommand {
    if (Get-Command "npm.cmd" -ErrorAction SilentlyContinue) {
        return "npm.cmd"
    }
    if (Get-Command "npm" -ErrorAction SilentlyContinue) {
        return "npm"
    }

    throw "npm is required to run dashboard tooling"
}

function Invoke-DashboardChecks {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot
    )

    $dashboardRoot = Join-Path $RepoRoot "ui\dashboard"
    if (-not (Test-Path $dashboardRoot)) {
        return
    }

    $packageLockPath = Join-Path $dashboardRoot "package-lock.json"
    if (-not (Test-Path $packageLockPath)) {
        throw "Dashboard checks require $packageLockPath"
    }

    Push-Location $dashboardRoot
    try {
        $npm = Resolve-NpmCommand

        & $npm ci
        if ($LASTEXITCODE -ne 0) {
            throw "npm ci failed for ui/dashboard"
        }
        & $npm run typecheck
        if ($LASTEXITCODE -ne 0) {
            throw "npm run typecheck failed for ui/dashboard"
        }
        & $npm run lint
        if ($LASTEXITCODE -ne 0) {
            throw "npm run lint failed for ui/dashboard"
        }
        & $npm test
        if ($LASTEXITCODE -ne 0) {
            throw "npm test failed for ui/dashboard"
        }
        & $npm run build
        if ($LASTEXITCODE -ne 0) {
            throw "npm run build failed for ui/dashboard"
        }
    } finally {
        Pop-Location
    }
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
    $runDir = Get-ChildItem -Path $outputRoot -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    $laneDir = if ($null -ne $runDir) {
        Join-Path $runDir.FullName "runtime-cert"
    } else {
        $null
    }
    $summaryPath = if ($null -ne $laneDir) {
        Join-Path $laneDir "summary.json"
    } else {
        $null
    }
    $summaryMarkdownPath = if ($null -ne $laneDir) {
        Join-Path $laneDir "summary.md"
    } else {
        $null
    }
    if ($LASTEXITCODE -ne 0) {
        if ($null -ne $runDir) {
            if (Test-Path $summaryMarkdownPath) {
                Write-Warning "Runtime smoke summary:"
                Get-Content -Path $summaryMarkdownPath | ForEach-Object { Write-Warning $_ }
            }
            if (Test-Path $summaryPath) {
                $summary = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
                foreach ($result in @($summary.results | Where-Object { -not $_.passed })) {
                    Write-Warning "Runtime smoke failure detail: $($result.id)"
                    Write-Warning "Summary: $($result.summary)"
                    $stdoutPath = $result.artifacts.stdout
                    $stderrPath = $result.artifacts.stderr
                    if ($stdoutPath -and (Test-Path $stdoutPath)) {
                        Write-Warning "stdout:"
                        Get-Content -Path $stdoutPath | ForEach-Object { Write-Warning $_ }
                    }
                    if ($stderrPath -and (Test-Path $stderrPath)) {
                        Write-Warning "stderr:"
                        Get-Content -Path $stderrPath | ForEach-Object { Write-Warning $_ }
                    }
                }
            }
        }
        throw "Runtime smoke run failed"
    }

    if ($null -eq $runDir) {
        throw "Runtime smoke did not produce an output directory"
    }

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
    Invoke-Step "dashboard checks" { Invoke-DashboardChecks -RepoRoot $repoRoot }
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
    Invoke-Step "workspace dependency drift" {
        Invoke-WorkspaceDependencyDriftCheck -RepoRoot $repoRoot
    }
    Invoke-Step "release gate script tests" {
        Invoke-ReleaseGateScriptTests -RepoRoot $repoRoot
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
