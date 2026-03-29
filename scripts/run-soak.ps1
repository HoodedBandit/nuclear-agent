param(
    [Parameter(Mandatory = $true)]
    [string]$Token,
    [string]$BaseUrl = "http://127.0.0.1:42690",
    [int]$Iterations = 30,
    [int]$DelayMs = 1000,
    [string]$Workspace = "",
    [string]$OutputRoot = ""
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $repoRoot "scripts\run-soak.cjs"

$args = @(
    $scriptPath,
    "--token", $Token,
    "--base-url", $BaseUrl,
    "--iterations", $Iterations,
    "--delay-ms", $DelayMs
)

if ($Workspace) {
    $args += @("--workspace", $Workspace)
}

if ($OutputRoot) {
    $args += @("--output-root", $OutputRoot)
}

node @args
