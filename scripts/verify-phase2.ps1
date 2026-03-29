param(
    [string]$BinaryPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Resolve-PythonCommand {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "python"
            Arguments  = @("-u")
        }
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "py"
            Arguments  = @("-3", "-u")
        }
    }
    throw "Python is required to run the Phase 2 smoke verification."
}

function Resolve-BinaryPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [string]$RequestedPath
    )

    if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        return (Resolve-Path $RequestedPath).Path
    }

    $candidates = @(
        (Join-Path $RepoRoot "target\verify-workspace\release\nuclear.exe"),
        (Join-Path $RepoRoot "target\release\nuclear.exe"),
        (Join-Path $RepoRoot "target\debug\nuclear.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw "Could not find a built nuclear.exe. Run verify-workspace.ps1 first or pass -BinaryPath."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$python = Resolve-PythonCommand
$resolvedBinaryPath = Resolve-BinaryPath -RepoRoot $repoRoot -RequestedPath $BinaryPath
$smokeScenarioRoot = Join-Path $repoRoot "target\phase2-smoke\windows"
$matrixScenarioRoot = Join-Path $repoRoot "target\phase2-matrix\windows"

& $python.Executable @($python.Arguments) `
    (Join-Path $PSScriptRoot "phase2_smoke.py") `
    --binary-path $resolvedBinaryPath `
    --repo-root $repoRoot `
    --scenario-root $smokeScenarioRoot

if ($LASTEXITCODE -ne 0) {
    throw "Phase 2 smoke verification failed"
}

& $python.Executable @($python.Arguments) `
    (Join-Path $PSScriptRoot "phase2_matrix.py") `
    --binary-path $resolvedBinaryPath `
    --repo-root $repoRoot `
    --scenario-root $matrixScenarioRoot

if ($LASTEXITCODE -ne 0) {
    throw "Phase 2 certification matrix failed"
}
