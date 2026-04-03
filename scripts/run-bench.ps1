param(
    [string]$TaskFile = ".\\benchmarks\\coding-smoke\\tasks.jsonl",
    [string]$BinaryPath = "",
    [string]$OutputRoot = "",
    [switch]$BootstrapProfile,
    [string]$BootstrapRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$arguments = @{
    Lane = "analysis-smoke"
    TaskFile = $TaskFile
}
if ($BinaryPath) { $arguments["BinaryPath"] = $BinaryPath }
if ($OutputRoot) { $arguments["OutputRoot"] = $OutputRoot }

& (Join-Path $PSScriptRoot "run-harness.ps1") @arguments
exit $LASTEXITCODE
