function Get-RepoRoot {
    param([string]$ScriptRoot)

    Split-Path -Parent $ScriptRoot
}

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Action
    )

    Write-Host "`n==> $Label" -ForegroundColor Cyan
    & $Action
    if (-not $?) {
        throw "Step failed: $Label"
    }
}

function Write-Step {
    param([string]$Message)

    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Resolve-PythonCommand {
    param(
        [string]$Purpose = "run Python tooling",
        [string[]]$ExtraArguments = @()
    )

    if (Get-Command python -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "python"
            Arguments  = @($ExtraArguments)
        }
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        return [pscustomobject]@{
            Executable = "py"
            Arguments  = @("-3") + @($ExtraArguments)
        }
    }

    throw "Python is required to $Purpose."
}

function Resolve-NpmCommand {
    if (Get-Command "npm.cmd" -ErrorAction SilentlyContinue) {
        return "npm.cmd"
    }
    if (Get-Command "npm" -ErrorAction SilentlyContinue) {
        return "npm"
    }

    throw "npm is required to run dashboard tooling."
}

function Remove-PathWithRetry {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [switch]$Recurse,
        [int]$MaxAttempts = 6,
        [int]$InitialDelayMs = 150
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    $delay = $InitialDelayMs
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            if ($Recurse) {
                Remove-Item -LiteralPath $Path -Recurse -Force
            } else {
                Remove-Item -LiteralPath $Path -Force
            }
            return
        } catch {
            if ($attempt -ge $MaxAttempts) {
                throw
            }
            Start-Sleep -Milliseconds $delay
            $delay = [Math]::Min($delay * 2, 2000)
        }
    }
}
