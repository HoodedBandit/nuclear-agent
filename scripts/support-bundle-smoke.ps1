param(
    [string]$BinaryPath = "",
    [string]$OutputRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "common.ps1")

$repoRoot = Get-RepoRoot $PSScriptRoot
$resolvedBinaryPath = if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
    Join-Path $repoRoot "target\verify-workspace\release\nuclear.exe"
} else {
    $BinaryPath
}
$scenarioRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    Join-Path $repoRoot "target\support-bundle-smoke"
} else {
    $OutputRoot
}

$pythonCommand = Resolve-PythonCommand -Purpose "run the support bundle smoke test"
& $pythonCommand.Executable @($pythonCommand.Arguments) `
    (Join-Path $PSScriptRoot "support_bundle_smoke.py") `
    --binary-path $resolvedBinaryPath `
    --repo-root $repoRoot `
    --scenario-root $scenarioRoot
exit $LASTEXITCODE
