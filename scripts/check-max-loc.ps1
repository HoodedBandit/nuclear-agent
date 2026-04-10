param(
    [int]$MaxLines = 2000,
    [string]$BaselineFile = (Join-Path $PSScriptRoot "max-loc-baseline.txt")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot

function Get-BaselineLimits {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $limits = @{}
    if (-not (Test-Path $Path)) {
        return $limits
    }

    foreach ($rawLine in Get-Content $Path) {
        $line = $rawLine.Trim()
        if (-not $line -or $line.StartsWith("#")) {
            continue
        }

        $parts = $line -split "\t", 2
        if ($parts.Count -ne 2) {
            throw "Invalid LOC baseline entry: '$rawLine'"
        }

        $limits[$parts[0]] = [int]$parts[1]
    }

    return $limits
}

function Get-NormalizedRelativePath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Root,
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $fullRoot = [System.IO.Path]::GetFullPath($Root)
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith($fullRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $fullPath.Substring($fullRoot.Length).TrimStart('\', '/').Replace("\", "/")
    }

    return $fullPath.Replace("\", "/")
}

Push-Location $repoRoot
try {
    $limits = Get-BaselineLimits -Path $BaselineFile
    $extensions = @(".rs", ".js", ".cjs", ".mjs", ".ts", ".tsx")
    $scanRoots = @(
        (Join-Path $repoRoot "crates"),
        (Join-Path $repoRoot "ui\dashboard\src")
    )
    $offenders = @()

    foreach ($scanRoot in $scanRoots) {
        if (-not (Test-Path $scanRoot)) {
            continue
        }

        Get-ChildItem -Path $scanRoot -Recurse -File |
            Where-Object { $extensions -contains $_.Extension.ToLowerInvariant() } |
            ForEach-Object {
                $relativePath = Get-NormalizedRelativePath -Root $repoRoot -Path $_.FullName
                $lineCount = [System.IO.File]::ReadAllLines($_.FullName).Length
                $limit = if ($limits.ContainsKey($relativePath)) { $limits[$relativePath] } else { $MaxLines }

                if ($lineCount -gt $limit) {
                    $offenders += [PSCustomObject]@{
                        Lines = $lineCount
                        Limit = $limit
                        Path  = $relativePath
                    }
                }
            }
    }

    if ($offenders.Count -eq 0) {
        return
    }

    foreach ($offender in $offenders | Sort-Object Lines -Descending) {
        Write-Output "$($offender.Lines)`t$($offender.Limit)`t$($offender.Path)"
    }
    exit 1
}
finally {
    Pop-Location
}
