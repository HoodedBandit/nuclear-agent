param(
    [string]$PackageRoot = ".\\target\\release\\package",
    [string]$PackageManifest = "",
    [string]$RuntimeCertRoot = ".\\target\\verify-ga\\runtime-cert",
    [string]$RuntimeCertSummary = "",
    [string]$CodingDeterministicRoot = ".\\target\\verify-ga\\coding-deterministic",
    [string]$CodingDeterministicSummary = "",
    [string]$CodingReferenceRoot = ".\\target\\finalize-release\\coding-reference",
    [string]$CodingReferenceSummary = "",
    [string]$AnalysisSmokeRoot = ".\\target\\harness\\analysis-smoke",
    [string]$AnalysisSmokeSummary = "",
    [string]$SoakRoot = ".\\target\\soak",
    [string]$SoakSummary = "",
    [switch]$RequireCodingReference,
    [string]$OutputRoot = ".\\target\\release-records",
    [string]$NotesFile = ".\\docs\\ga-release-notes.md",
    [string]$ChecklistFile = ".\\docs\\release-checklist.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

. (Join-Path $PSScriptRoot "common.ps1")

$pythonCommand = Resolve-PythonCommand -Purpose "write release records"
$scriptPath = Join-Path $PSScriptRoot "write_release_record.py"
$arguments = @(
    $scriptPath,
    "--package-root", $PackageRoot,
    "--runtime-cert-root", $RuntimeCertRoot,
    "--coding-deterministic-root", $CodingDeterministicRoot,
    "--coding-reference-root", $CodingReferenceRoot,
    "--analysis-smoke-root", $AnalysisSmokeRoot,
    "--soak-root", $SoakRoot,
    "--output-root", $OutputRoot,
    "--notes-file", $NotesFile,
    "--checklist-file", $ChecklistFile
)

if ($PackageManifest) { $arguments += @("--package-manifest", $PackageManifest) }
if ($RuntimeCertSummary) { $arguments += @("--runtime-cert-summary", $RuntimeCertSummary) }
if ($CodingDeterministicSummary) { $arguments += @("--coding-deterministic-summary", $CodingDeterministicSummary) }
if ($CodingReferenceSummary) { $arguments += @("--coding-reference-summary", $CodingReferenceSummary) }
if ($AnalysisSmokeSummary) { $arguments += @("--analysis-smoke-summary", $AnalysisSmokeSummary) }
if ($SoakSummary) { $arguments += @("--soak-summary", $SoakSummary) }
if ($RequireCodingReference) { $arguments += "--require-coding-reference" }

& $pythonCommand.Executable @($pythonCommand.Arguments) @arguments
exit $LASTEXITCODE
