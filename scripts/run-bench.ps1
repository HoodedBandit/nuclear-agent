param(
    [string]$TaskFile = ".\\benchmarks\\coding-smoke\\tasks.jsonl",
    [string]$BinaryPath = "",
    [string]$OutputRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

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
    throw "Python is required to run benchmarks."
}

$pythonCommand = Resolve-PythonCommand
$scriptPath = Join-Path $PSScriptRoot "run_bench.py"
$arguments = @($scriptPath, "--task-file", $TaskFile)

if ($BinaryPath) {
    $arguments += @("--binary-path", $BinaryPath)
}
if ($OutputRoot) {
    $arguments += @("--output-root", $OutputRoot)
}

& $pythonCommand.Executable @($pythonCommand.Arguments) @arguments
exit $LASTEXITCODE
