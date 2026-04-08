param(
    [switch]$SkipE2E,
    [switch]$SkipReleaseEval,
    [switch]$SkipSoak,
    [switch]$SkipSigning,
    [string]$Token = "",
    [string]$BaseUrl = "http://127.0.0.1:42690",
    [string]$Workspace = "",
    [string]$PackageOutputRoot = "",
    [string]$ReleaseRecordOutputRoot = "",
    [string]$SoakOutputRoot = ""
)

$params = @{
    Token = $Token
    BaseUrl = $BaseUrl
    Workspace = $Workspace
}
if ($SkipE2E) {
    $params["SkipE2E"] = $true
}
if ($SkipReleaseEval) {
    $params["SkipReleaseEval"] = $true
}
if ($SkipSoak) {
    $params["SkipSoak"] = $true
}
if ($SkipSigning) {
    $params["SkipSigning"] = $true
}
if ($PackageOutputRoot) {
    $params["PackageOutputRoot"] = $PackageOutputRoot
}
if ($ReleaseRecordOutputRoot) {
    $params["ReleaseRecordOutputRoot"] = $ReleaseRecordOutputRoot
}
if ($SoakOutputRoot) {
    $params["SoakOutputRoot"] = $SoakOutputRoot
}

& (Join-Path $PSScriptRoot "finalize-release.ps1") @params
exit $LASTEXITCODE
