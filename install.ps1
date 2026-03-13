param(
    [string]$InstallDir = $env:AUTISM_INSTALL_DIR,
    [switch]$NoPathPersist,
    [switch]$PreferSourceBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Path-Contains {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $needle = $Entry.TrimEnd("\")
    foreach ($segment in $PathValue.Split(";", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if ($segment.TrimEnd("\") -ieq $needle) {
            return $true
        }
    }

    return $false
}

function Resolve-ScriptRoot {
    if ($PSScriptRoot) {
        return $PSScriptRoot
    }

    return Split-Path -Parent $PSCommandPath
}

function Choose-DefaultInstallDir {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $candidates = @()

    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $candidates += (Join-Path $env:USERPROFILE ".cargo\bin")
        $candidates += (Join-Path $env:USERPROFILE ".local\bin")
    }

    foreach ($candidate in $candidates) {
        $currentPathHasCandidate = Path-Contains -PathValue $env:Path -Entry $candidate
        $userPathHasCandidate = Path-Contains -PathValue $userPath -Entry $candidate
        if ($currentPathHasCandidate -or $userPathHasCandidate) {
            return $candidate
        }
    }

    return Join-Path $env:LOCALAPPDATA "Programs\NuclearAI\Autism\bin"
}

function Resolve-SourceRoot {
    param([string]$Root)

    $packageSource = Join-Path $Root "source"
    if (Test-Path (Join-Path $packageSource "Cargo.toml")) {
        return $packageSource
    }

    if (Test-Path (Join-Path $Root "Cargo.toml")) {
        return $Root
    }

    throw "Could not locate the project source directory."
}

function Resolve-BundledBinary {
    param([string]$Root)

    $candidates = @(
        (Join-Path $Root "bin\windows-x64\autism.exe"),
        (Join-Path $Root "target\release\autism.exe")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

function Get-CargoBinDir {
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_HOME)) {
        return (Join-Path $env:CARGO_HOME "bin")
    }

    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        return (Join-Path $env:USERPROFILE ".cargo\bin")
    }

    return (Join-Path $HOME ".cargo\bin")
}

function Add-ToProcessPath {
    param([string]$Entry)

    if (-not (Path-Contains -PathValue $env:Path -Entry $Entry)) {
        if ([string]::IsNullOrWhiteSpace($env:Path)) {
            $env:Path = $Entry
        } else {
            $env:Path = "$Entry;$env:Path"
        }
    }
}

function Get-RustupArchTriple {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($arch.ToUpperInvariant()) {
        "ARM64" { return "aarch64-pc-windows-msvc" }
        default { return "x86_64-pc-windows-msvc" }
    }
}

function Install-Rustup {
    $cargoBin = Get-CargoBinDir
    $rustupPath = Join-Path $cargoBin "rustup.exe"
    $cargoPath = Join-Path $cargoBin "cargo.exe"
    if ((Test-Path $rustupPath) -and (Test-Path $cargoPath)) {
        Add-ToProcessPath -Entry $cargoBin
        return
    }

    Write-Step "Rust toolchain not found; installing rustup"
    $triple = Get-RustupArchTriple
    $rustupUri = "https://static.rust-lang.org/rustup/dist/$triple/rustup-init.exe"
    $installerPath = Join-Path ([System.IO.Path]::GetTempPath()) ("rustup-init-" + [System.Guid]::NewGuid().ToString("N") + ".exe")
    try {
        Invoke-WebRequest -Uri $rustupUri -OutFile $installerPath
        & $installerPath -y --profile minimal --default-toolchain stable --no-modify-path
        if ($LASTEXITCODE -ne 0) {
            throw "rustup-init failed with exit code $LASTEXITCODE"
        }
    } finally {
        Remove-Item -Force $installerPath -ErrorAction SilentlyContinue
    }

    Add-ToProcessPath -Entry $cargoBin
}

function Ensure-Cargo {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cargo) {
        return $cargo.Source
    }

    Install-Rustup
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        $fallbackPath = Join-Path (Get-CargoBinDir) "cargo.exe"
        if (Test-Path $fallbackPath) {
            Add-ToProcessPath -Entry (Split-Path -Parent $fallbackPath)
            $cargo = Get-Command cargo -ErrorAction SilentlyContinue
        }
    }

    if (-not $cargo) {
        throw "cargo is still unavailable after rustup installation."
    }

    return $cargo.Source
}

function Unblock-PathTree {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return
    }

    try {
        if ((Get-Item $Path).PSIsContainer) {
            Get-ChildItem -Path $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
                Unblock-File -ErrorAction SilentlyContinue
        } else {
            Unblock-File -Path $Path -ErrorAction SilentlyContinue
        }
    } catch {
    }
}

function Get-VersionOutput {
    param([string]$BinaryPath)

    $versionOutput = & $BinaryPath --version 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        throw "Installed binary failed its version check with exit code $LASTEXITCODE.`n$versionOutput"
    }
    return $versionOutput.Trim()
}

function Test-ApplicationControlBlock {
    param([object]$ErrorRecord)

    $text = if ($ErrorRecord -is [System.Management.Automation.ErrorRecord]) {
        ($ErrorRecord | Out-String)
    } else {
        [string]$ErrorRecord
    }

    return $text -match "Application Control policy has blocked this file" -or
        $text -match "AppLocker" -or
        $text -match "Smart App Control" -or
        $text -match "blocked this file"
}

function Copy-FileAtomic {
    param(
        [string]$Source,
        [string]$Destination
    )

    for ($attempt = 1; $attempt -le 20; $attempt++) {
        $tempPath = "$Destination.new"
        try {
            Copy-Item -Force $Source $tempPath
            Move-Item -Force $tempPath $Destination
            return
        } catch {
            Remove-Item -Force $tempPath -ErrorAction SilentlyContinue
            if ($attempt -eq 20) {
                throw
            }
            Start-Sleep -Milliseconds (250 * $attempt)
        }
    }
}

function Get-InstalledAutismProcesses {
    param([string]$BinaryPath)

    if (-not (Test-Path $BinaryPath)) {
        return @()
    }

    $normalized = [System.IO.Path]::GetFullPath($BinaryPath)
    Get-CimInstance Win32_Process -Filter "Name = 'autism.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -and
            ([System.IO.Path]::GetFullPath($_.ExecutablePath) -ieq $normalized)
        }
}

function Stop-InstalledAutismProcesses {
    param([string]$BinaryPath)

    if (-not (Test-Path $BinaryPath)) {
        return
    }

    Write-Step "Stopping any running autism processes from the install directory"
    try {
        & $BinaryPath daemon stop *> $null
    } catch {
    }

    Start-Sleep -Milliseconds 750
    $processes = @(Get-InstalledAutismProcesses -BinaryPath $BinaryPath)
    foreach ($process in $processes) {
        try {
            Stop-Process -Id $process.ProcessId -Force -ErrorAction Stop
        } catch {
        }
    }

    if ($processes.Count -gt 0) {
        Start-Sleep -Milliseconds 750
    }
}

function Write-CommandShim {
    param([string]$InstallDir)

    $shimPath = Join-Path $InstallDir "autism.cmd"
    $lines = @(
        "@echo off",
        """%~dp0autism.exe"" %*"
    )
    Set-Content -Path $shimPath -Value $lines -Encoding Ascii
}

function Build-BinaryFromSource {
    param(
        [string]$SourceRoot,
        [string]$TargetBinary
    )

    $cargoPath = Ensure-Cargo

    Write-Step "Building autism.exe from source"
    Push-Location $SourceRoot
    try {
        & $cargoPath build --release --bin autism
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
    } finally {
        Pop-Location
    }

    $builtBinary = Join-Path $SourceRoot "target\release\autism.exe"
    if (-not (Test-Path $builtBinary)) {
        throw "Cargo reported success but $builtBinary was not found."
    }

    Copy-FileAtomic -Source $builtBinary -Destination $TargetBinary
}

function Ensure-InstallDir {
    param([string]$TargetDir)

    if (-not (Test-Path $TargetDir)) {
        New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
    }
}

function Write-InstallerErrorLog {
    param(
        [System.Management.Automation.ErrorRecord]$ErrorRecord,
        [string]$Root
    )

    $logPath = Join-Path $Root "install-error.log"
    $details = @(
        "Timestamp: $(Get-Date -Format o)",
        "Message: $($ErrorRecord.Exception.Message)",
        "Category: $($ErrorRecord.CategoryInfo)",
        "ScriptStackTrace:",
        $ErrorRecord.ScriptStackTrace,
        "",
        "FullError:",
        ($ErrorRecord | Format-List * -Force | Out-String)
    )
    Set-Content -Path $logPath -Value $details -Encoding UTF8
    return $logPath
}

try {
    if ($env:OS -ne "Windows_NT") {
        throw "install.ps1 supports Windows only. Use ./install on Linux."
    }

    $scriptRoot = Resolve-ScriptRoot
    $sourceRoot = Resolve-SourceRoot -Root $scriptRoot
    Unblock-PathTree -Path $scriptRoot
    $errorLogPath = Join-Path $scriptRoot "install-error.log"
    if (Test-Path $errorLogPath) {
        Remove-Item -Force $errorLogPath
    }

    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Choose-DefaultInstallDir
    }

    $binaryPath = Join-Path $InstallDir "autism.exe"
    $bundledBinary = Resolve-BundledBinary -Root $scriptRoot

    Write-Step "Installing autism CLI"
    Write-Step "Install directory: $InstallDir"
    Ensure-InstallDir -TargetDir $InstallDir
    Stop-InstalledAutismProcesses -BinaryPath $binaryPath

    $usedBundledBinary = $false
    if ($bundledBinary -and -not $PreferSourceBuild) {
        Write-Step "Using bundled Windows binary"
        Copy-FileAtomic -Source $bundledBinary -Destination $binaryPath
        Unblock-PathTree -Path $binaryPath
        $usedBundledBinary = $true
    } else {
        Build-BinaryFromSource -SourceRoot $sourceRoot -TargetBinary $binaryPath
        Unblock-PathTree -Path $binaryPath
    }

    Write-CommandShim -InstallDir $InstallDir

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $updatedFuturePath = $false
    if (-not $NoPathPersist -and -not (Path-Contains -PathValue $userPath -Entry $InstallDir)) {
        $newUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
            $InstallDir
        } else {
            "$InstallDir;$userPath"
        }
        [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
        $updatedFuturePath = $true
    }

    if (-not (Path-Contains -PathValue $env:Path -Entry $InstallDir)) {
        if ([string]::IsNullOrWhiteSpace($env:Path)) {
            $env:Path = $InstallDir
        } else {
            $env:Path = "$InstallDir;$env:Path"
        }
    }

    try {
        $version = Get-VersionOutput -BinaryPath $binaryPath
    } catch {
        if ($usedBundledBinary -and (Test-ApplicationControlBlock -ErrorRecord $_)) {
            Write-Step "Bundled binary appears blocked by Windows policy; falling back to a local source build"
            Build-BinaryFromSource -SourceRoot $sourceRoot -TargetBinary $binaryPath
            Unblock-PathTree -Path $binaryPath
            $version = Get-VersionOutput -BinaryPath $binaryPath
        } else {
            throw
        }
    }

    if ($updatedFuturePath) {
        Write-Step "Updated the user PATH for future terminal sessions"
    } elseif ($NoPathPersist) {
        Write-Step "Skipped persistent PATH changes for this install run"
    } else {
        Write-Step "Install directory is already configured on PATH"
    }

    Write-Step "Installed version: $version"
    Write-Step "Run: autism"
    Write-Step "If an already-open terminal does not recognize the command, close it and open a new terminal window."
} catch {
    $root = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $PSCommandPath }
    $logPath = Write-InstallerErrorLog -ErrorRecord $_ -Root $root
    Write-Error "Installation failed. See $logPath for details."
    exit 1
}
