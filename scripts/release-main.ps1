param(
    [string]$Remote = "origin",
    [string]$Branch = "main",
    [switch]$SkipLocalVerify,
    [switch]$SkipPush,
    [switch]$SkipRemoteVerify,
    [switch]$SkipFinalize,
    [int]$TimeoutSeconds = 3600,
    [int]$PollIntervalSeconds = 15
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "common.ps1")

function Assert-CleanWorktree {
    $status = git status --porcelain
    if ($LASTEXITCODE -ne 0) {
        throw "git status failed"
    }
    if (-not [string]::IsNullOrWhiteSpace(($status | Out-String))) {
        throw "Release helper requires a clean worktree. Commit or stash changes first."
    }
}

function Assert-Branch {
    param([string]$ExpectedBranch)

    $currentBranch = (git rev-parse --abbrev-ref HEAD).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "git rev-parse failed"
    }
    if ($currentBranch -ne $ExpectedBranch) {
        throw "Release helper must run from '$ExpectedBranch'. Current branch is '$currentBranch'."
    }
}

function Assert-Command {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Purpose
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name is required to $Purpose."
    }
}

$repoRoot = Get-RepoRoot $PSScriptRoot

Push-Location $repoRoot
try {
    Invoke-Step "verify clean worktree" {
        Assert-CleanWorktree
    }

    Invoke-Step "verify current branch" {
        Assert-Branch -ExpectedBranch $Branch
    }

    if (-not $SkipLocalVerify) {
        Invoke-Step "local GA verification" {
            & (Join-Path $PSScriptRoot "verify-ga.ps1")
        }
    }

    if (-not $SkipPush) {
        Invoke-Step "push $Branch to $Remote" {
            git push $Remote $Branch
            if ($LASTEXITCODE -ne 0) {
                throw "git push failed"
            }
        }
    }

    if (-not $SkipRemoteVerify -or -not $SkipFinalize) {
        Assert-Command -Name "gh" -Purpose "query GitHub Actions and dispatch the release workflow"
        $python = Resolve-PythonCommand -Purpose "check GitHub verification state"
        $repo = (& gh repo view --json nameWithOwner --jq ".nameWithOwner").Trim()
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($repo)) {
            throw "Unable to resolve GitHub repository slug via gh."
        }
        $sha = (git rev-parse HEAD).Trim()
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($sha)) {
            throw "Unable to resolve HEAD commit SHA."
        }

        if (-not $SkipRemoteVerify) {
            Invoke-Step "wait for remote ga-verify success" {
                $token = (& gh auth token).Trim()
                if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($token)) {
                    throw "Unable to resolve a GitHub token from gh auth token."
                }
                $previousToken = $env:GITHUB_TOKEN
                $env:GITHUB_TOKEN = $token
                try {
                    & $python.Executable @($python.Arguments) `
                        (Join-Path $PSScriptRoot "require_green_ga.py") `
                        --repo $repo `
                        --sha $sha `
                        --branch $Branch `
                        --wait `
                        --timeout-seconds $TimeoutSeconds `
                        --poll-interval-seconds $PollIntervalSeconds
                    if ($LASTEXITCODE -ne 0) {
                        throw "Remote ga-verify did not pass."
                    }
                } finally {
                    if ($null -eq $previousToken) {
                        Remove-Item Env:GITHUB_TOKEN -ErrorAction SilentlyContinue
                    } else {
                        $env:GITHUB_TOKEN = $previousToken
                    }
                }
            }
        }

        if (-not $SkipFinalize) {
            Invoke-Step "dispatch finalize-release" {
                gh workflow run finalize-release.yml --ref $Branch
                if ($LASTEXITCODE -ne 0) {
                    throw "finalize-release workflow dispatch failed"
                }
            }
        }
    }
} finally {
    Pop-Location
}
