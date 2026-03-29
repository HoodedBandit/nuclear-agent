param(
    [string]$PackageRoot = ".\\target\\phase3\\package",
    [string]$PackageManifest = "",
    [string]$BenchmarkSmokeRoot = ".\\target\\verify-workspace\\benchmarks-smoke",
    [string]$BenchmarkSmokeSummary = "",
    [string]$ReleaseEvalRoot = ".\\target\\verify-beta\\release-eval",
    [string]$ReleaseEvalSummary = "",
    [string]$SoakRoot = ".\\target\\soak",
    [string]$SoakSummary = "",
    [string]$OutputRoot = ".\\target\\release-records"
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
    throw "Python is required to write release records."
}

$pythonCommand = Resolve-PythonCommand
$scriptPath = Join-Path $PSScriptRoot "write_release_record.py"
$arguments = @(
    $scriptPath,
    "--package-root", $PackageRoot,
    "--benchmark-smoke-root", $BenchmarkSmokeRoot,
    "--release-eval-root", $ReleaseEvalRoot,
    "--soak-root", $SoakRoot,
    "--output-root", $OutputRoot
)

if ($PackageManifest) {
    $arguments += @("--package-manifest", $PackageManifest)
}
if ($BenchmarkSmokeSummary) {
    $arguments += @("--benchmark-smoke-summary", $BenchmarkSmokeSummary)
}
if ($ReleaseEvalSummary) {
    $arguments += @("--release-eval-summary", $ReleaseEvalSummary)
}
if ($SoakSummary) {
    $arguments += @("--soak-summary", $SoakSummary)
}

& $pythonCommand.Executable @($pythonCommand.Arguments) @arguments
exit $LASTEXITCODE
