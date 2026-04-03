param(
    [string]$BinaryPath = "",
    [string]$OutputRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-PythonCommand {
    if (Get-Command python -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "python"
            Arguments  = @()
        }
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "py"
            Arguments  = @("-3")
        }
    }
    throw "Python is required to run the support bundle smoke test."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
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

$pythonCommand = Resolve-PythonCommand
& $pythonCommand.Executable @($pythonCommand.Arguments) `
    (Join-Path $PSScriptRoot "support_bundle_smoke.py") `
    --binary-path $resolvedBinaryPath `
    --repo-root $repoRoot `
    --scenario-root $scenarioRoot
exit $LASTEXITCODE
