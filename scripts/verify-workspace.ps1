$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    Write-Host "`n==> $Label" -ForegroundColor Cyan
    & $Action
}

function Invoke-OptionalCargoTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Tool,
        [Parameter(Mandatory = $true)]
        [string[]]$Args
    )

    $cargoList = cargo --list
    if ($cargoList -notmatch "^\s+$Tool\s") {
        Write-Warning "cargo-$Tool is not installed; skipping. Install with: cargo install cargo-$Tool --locked"
        return
    }

    & cargo $Tool @Args
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    Invoke-Step "cargo check --workspace" { cargo check --workspace }
    Invoke-Step "cargo test --workspace" { cargo test --workspace }
    Invoke-Step "cargo build --release --bin autism" { cargo build --release --bin autism }
    Invoke-Step "cargo tree --workspace --duplicates" { cargo tree --workspace --duplicates }
    Invoke-Step "cargo audit" { Invoke-OptionalCargoTool -Tool "audit" -Args @() }
    Invoke-Step "cargo deny check advisories licenses bans" {
        Invoke-OptionalCargoTool -Tool "deny" -Args @("check", "advisories", "licenses", "bans")
    }
    Invoke-Step "cargo outdated -R" { Invoke-OptionalCargoTool -Tool "outdated" -Args @("-R") }
}
finally {
    Pop-Location
}
