param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("runtime-cert", "coding-deterministic", "coding-reference", "analysis-smoke", "soak")]
    [string]$Lane,
    [string]$BinaryPath = "",
    [string]$OutputRoot = "",
    [string]$TaskFile = "",
    [string]$Profile = "",
    [string]$TaskFilter = "",
    [string]$Alias = "",
    [string]$ProviderId = "",
    [string]$Model = "",
    [string]$ProviderKind = "",
    [string]$BaseUrl = "",
    [string]$ApiKeyEnv = "",
    [string]$Token = "",
    [string]$SoakBaseUrl = "http://127.0.0.1:42690",
    [string]$Workspace = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "common.ps1")

$pythonCommand = Resolve-PythonCommand -Purpose "run the harness"
$scriptPath = Join-Path $PSScriptRoot "run_harness.py"
$arguments = @($scriptPath, "--lane", $Lane)

if ($BinaryPath) { $arguments += @("--binary-path", $BinaryPath) }
if ($OutputRoot) { $arguments += @("--output-root", $OutputRoot) }
if ($TaskFile) { $arguments += @("--task-file", $TaskFile) }
if ($Profile) { $arguments += @("--profile", $Profile) }
if ($TaskFilter) { $arguments += @("--task-filter", $TaskFilter) }
if ($Alias) { $arguments += @("--alias", $Alias) }
if ($ProviderId) { $arguments += @("--provider-id", $ProviderId) }
if ($Model) { $arguments += @("--model", $Model) }
if ($ProviderKind) { $arguments += @("--provider-kind", $ProviderKind) }
if ($BaseUrl) { $arguments += @("--base-url", $BaseUrl) }
if ($ApiKeyEnv) { $arguments += @("--api-key-env", $ApiKeyEnv) }
if ($Token) { $arguments += @("--token", $Token) }
if ($SoakBaseUrl) { $arguments += @("--soak-base-url", $SoakBaseUrl) }
if ($Workspace) { $arguments += @("--workspace", $Workspace) }

& $pythonCommand.Executable @($pythonCommand.Arguments) @arguments
exit $LASTEXITCODE

