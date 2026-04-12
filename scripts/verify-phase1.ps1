param(
    [string]$BinaryPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "common.ps1")

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

$repoRoot = Get-RepoRoot $PSScriptRoot
$python = Resolve-PythonCommand -Purpose "run the Phase 1 smoke verification" -ExtraArguments @("-u")
$resolvedBinaryPath = Resolve-BinaryPath -RepoRoot $repoRoot -RequestedPath $BinaryPath
$scenarioRoot = Join-Path $repoRoot "target\phase1-smoke\windows"

& $python.Executable @($python.Arguments) `
    (Join-Path $PSScriptRoot "phase1_smoke.py") `
    --binary-path $resolvedBinaryPath `
    --repo-root $repoRoot `
    --scenario-root $scenarioRoot

if ($LASTEXITCODE -ne 0) {
    throw "Phase 1 smoke verification failed"
}
