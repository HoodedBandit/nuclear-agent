param(
    [switch]$SkipE2E,
    [switch]$SkipReleaseEval,
    [string]$TaskFile = ".\\harness\\tasks\\coding\\tasks.json",
    [string]$OutputRoot = ""
)

$params = @{
    TaskFile = $TaskFile
}
if ($SkipE2E) {
    $params["SkipE2E"] = $true
}
if ($SkipReleaseEval) {
    $params["SkipReleaseEval"] = $true
}
if ($OutputRoot) {
    $params["OutputRoot"] = $OutputRoot
}

& (Join-Path $PSScriptRoot "verify-ga.ps1") @params
exit $LASTEXITCODE
