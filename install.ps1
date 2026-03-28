param(
    [string]$InstallDir,
    [switch]$NoPathPersist,
    [switch]$PreferSourceBuild,
    [switch]$SkipPlaywrightSetup
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

function Get-ManagedDependencyRoot {
    if (-not [string]::IsNullOrWhiteSpace($env:NUCLEAR_DEPENDENCY_ROOT)) {
        return $env:NUCLEAR_DEPENDENCY_ROOT
    }

    if (-not [string]::IsNullOrWhiteSpace($env:AUTISM_DEPENDENCY_ROOT)) {
        return $env:AUTISM_DEPENDENCY_ROOT
    }

    return Join-Path $env:LOCALAPPDATA "Programs\NuclearAI\Autism\deps"
}

function Get-ManagedPlaywrightRoot {
    return Join-Path (Get-ManagedDependencyRoot) "ms-playwright"
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
        (Join-Path $Root "bin\windows-x64\nuclear.exe"),
        (Join-Path $Root "bin\windows-x64\autism.exe"),
        (Join-Path $Root "target\release\nuclear.exe"),
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

function Get-NodeCommand {
    param([string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $managedNodeHome = Get-ManagedNodeHome
    if ($managedNodeHome) {
        Add-ToProcessPath -Entry $managedNodeHome
        $command = Get-Command $Name -ErrorAction SilentlyContinue
        if ($command) {
            return $command.Source
        }
    }

    return $null
}

function Get-NodeArchTag {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($arch.ToUpperInvariant()) {
        "ARM64" { return "arm64" }
        default { return "x64" }
    }
}

function Get-ManagedNodeHome {
    $dependencyRoot = Get-ManagedDependencyRoot
    if (-not (Test-Path $dependencyRoot)) {
        return $null
    }

    $candidates = Get-ChildItem -Path $dependencyRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like "node-v*-win-*" } |
        Sort-Object Name -Descending

    foreach ($candidate in $candidates) {
        if (Test-Path (Join-Path $candidate.FullName "node.exe")) {
            return $candidate.FullName
        }
    }

    return $null
}

function Get-BundledNodeHome {
    param([string]$Root)

    $candidates = @(
        (Join-Path $Root "deps"),
        (Join-Path $Root "source\deps")
    )

    foreach ($candidateRoot in $candidates) {
        if (-not (Test-Path $candidateRoot)) {
            continue
        }

        $candidate = Get-ChildItem -Path $candidateRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "node-v*-win-*" } |
            Sort-Object Name -Descending |
            Select-Object -First 1
        if ($candidate -and (Test-Path (Join-Path $candidate.FullName "node.exe"))) {
            return $candidate.FullName
        }
    }

    return $null
}

function Copy-DirectoryContents {
    param(
        [string]$Source,
        [string]$Destination
    )

    Ensure-InstallDir -TargetDir $Destination
    Get-ChildItem -LiteralPath $Source -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $Destination $_.Name) -Recurse -Force
    }
}

function Get-NodeDownloadInfo {
    $archTag = Get-NodeArchTag
    $zipTag = "win-$archTag-zip"
    Write-Step "Resolving portable Node.js runtime for Playwright setup"
    $releases = Invoke-RestMethod -Uri "https://nodejs.org/dist/index.json"
    $release = $releases |
        Where-Object { $_.lts -and $_.files -contains $zipTag } |
        Select-Object -First 1
    if (-not $release) {
        $release = $releases |
            Where-Object { $_.files -contains $zipTag } |
            Select-Object -First 1
    }
    if (-not $release) {
        throw "Could not find a portable Node.js release for $zipTag."
    }

    $version = [string]$release.version
    return @{
        Version = $version
        FolderName = "node-$version-win-$archTag"
        DownloadUri = "https://nodejs.org/dist/$version/node-$version-win-$archTag.zip"
    }
}

function Install-PortableNode {
    param([string]$BundleRoot)

    $existing = Get-ManagedNodeHome
    if ($existing) {
        Add-ToProcessPath -Entry $existing
        return $existing
    }

    $dependencyRoot = Get-ManagedDependencyRoot
    Ensure-InstallDir -TargetDir $dependencyRoot

    $bundledNode = $null
    if (-not [string]::IsNullOrWhiteSpace($BundleRoot)) {
        $bundledNode = Get-BundledNodeHome -Root $BundleRoot
    }
    if ($bundledNode) {
        $targetHome = Join-Path $dependencyRoot (Split-Path -Leaf $bundledNode)
        if (-not (Test-Path (Join-Path $targetHome "node.exe"))) {
            Write-Step "Node.js not found; using bundled portable Node.js runtime"
            if (Test-Path $targetHome) {
                Remove-Item -Recurse -Force $targetHome
            }
            Copy-DirectoryContents -Source $bundledNode -Destination $targetHome
        }
        Add-ToProcessPath -Entry $targetHome
        return $targetHome
    }

    $download = Get-NodeDownloadInfo
    $targetHome = Join-Path $dependencyRoot $download.FolderName
    if (Test-Path (Join-Path $targetHome "node.exe")) {
        Add-ToProcessPath -Entry $targetHome
        return $targetHome
    }

    Write-Step "Node.js not found; installing a managed portable Node.js runtime"
    $tempZip = Join-Path ([System.IO.Path]::GetTempPath()) ("node-runtime-" + [System.Guid]::NewGuid().ToString("N") + ".zip")
    $tempExtract = Join-Path ([System.IO.Path]::GetTempPath()) ("node-runtime-" + [System.Guid]::NewGuid().ToString("N"))
    try {
        Invoke-WebRequest -Uri $download.DownloadUri -OutFile $tempZip
        Expand-Archive -LiteralPath $tempZip -DestinationPath $tempExtract -Force
        $expandedRoot = Get-ChildItem -Path $tempExtract -Directory | Select-Object -First 1
        if (-not $expandedRoot) {
            throw "Portable Node.js archive did not contain an extracted root directory."
        }
        if (Test-Path $targetHome) {
            Remove-Item -Recurse -Force $targetHome
        }
        Move-Item -Path $expandedRoot.FullName -Destination $targetHome
    } finally {
        Remove-Item -Force $tempZip -ErrorAction SilentlyContinue
        Remove-Item -Recurse -Force $tempExtract -ErrorAction SilentlyContinue
    }

    Add-ToProcessPath -Entry $targetHome
    return $targetHome
}

function Ensure-NodeRuntime {
    param([string]$BundleRoot)

    $nodePath = Get-NodeCommand -Name "node"
    $npmPath = Get-NodeCommand -Name "npm"
    if ($nodePath -and $npmPath) {
        return @{
            Node = $nodePath
            Npm = $npmPath
            Managed = $false
        }
    }

    $managedNodeHome = Install-PortableNode -BundleRoot $BundleRoot
    $nodePath = Get-NodeCommand -Name "node"
    $npmPath = Get-NodeCommand -Name "npm"
    if (-not $nodePath -or -not $npmPath) {
        throw "Portable Node.js installation completed but node/npm are still unavailable."
    }

    return @{
        Node = $nodePath
        Npm = $npmPath
        Managed = $true
        Home = $managedNodeHome
    }
}

function Test-PlaywrightPackagePresent {
    param([string]$SourceRoot)

    $packageJsonPath = Join-Path $SourceRoot "package.json"
    if (-not (Test-Path $packageJsonPath)) {
        return $false
    }

    try {
        $package = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
    } catch {
        return $false
    }

    $dependencies = @{}
    $dependencyBlock = $package.PSObject.Properties["dependencies"]
    if ($dependencyBlock -and $dependencyBlock.Value) {
        foreach ($entry in $dependencyBlock.Value.PSObject.Properties) {
            $dependencies[$entry.Name] = [string]$entry.Value
        }
    }
    $devDependencyBlock = $package.PSObject.Properties["devDependencies"]
    if ($devDependencyBlock -and $devDependencyBlock.Value) {
        foreach ($entry in $devDependencyBlock.Value.PSObject.Properties) {
            $dependencies[$entry.Name] = [string]$entry.Value
        }
    }

    return $dependencies.ContainsKey("@playwright/test") -or
        $dependencies.ContainsKey("playwright")
}

function Get-LocalPlaywrightCommand {
    param([string]$SourceRoot)

    $binDir = Join-Path $SourceRoot "node_modules\.bin"
    $candidates = @(
        (Join-Path $binDir "playwright.cmd"),
        (Join-Path $binDir "playwright.ps1"),
        (Join-Path $binDir "playwright")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

function Get-BundledPlaywrightRoot {
    param([string]$Root)

    $candidates = @(
        (Join-Path $Root "deps\ms-playwright"),
        (Join-Path $Root "source\deps\ms-playwright")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

function Ensure-PlaywrightDependencies {
    param(
        [string]$SourceRoot,
        [string]$NpmPath
    )

    $playwrightPackage = Join-Path $SourceRoot "node_modules\playwright-core\package.json"
    $playwrightTestPackage = Join-Path $SourceRoot "node_modules\@playwright\test\package.json"
    if ((Test-Path $playwrightPackage) -and (Test-Path $playwrightTestPackage)) {
        return
    }

    $packageLockPath = Join-Path $SourceRoot "package-lock.json"
    $installArgs = if (Test-Path $packageLockPath) {
        @("ci", "--include=dev", "--no-fund", "--no-audit")
    } else {
        @("install", "--include=dev", "--no-fund", "--no-audit")
    }

    Write-Step "Installing local Playwright npm dependencies"
    Push-Location $SourceRoot
    try {
        & $npmPath @installArgs
        if ($LASTEXITCODE -ne 0) {
            throw "npm $($installArgs[0]) failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Invoke-Playwright {
    param(
        [string]$PlaywrightCommand,
        [string[]]$Arguments,
        [string]$BrowsersPath
    )

    $previousBrowsersPath = $env:PLAYWRIGHT_BROWSERS_PATH
    try {
        $env:PLAYWRIGHT_BROWSERS_PATH = $BrowsersPath
        & $PlaywrightCommand @Arguments 2>&1 | Out-String
    } finally {
        if ($null -eq $previousBrowsersPath) {
            Remove-Item Env:PLAYWRIGHT_BROWSERS_PATH -ErrorAction SilentlyContinue
        } else {
            $env:PLAYWRIGHT_BROWSERS_PATH = $previousBrowsersPath
        }
    }
}

function Get-PlaywrightInstallLocations {
    param(
        [string]$PlaywrightCommand,
        [string]$BrowsersPath
    )

    $output = Invoke-Playwright -PlaywrightCommand $PlaywrightCommand -Arguments @("install", "chromium", "--dry-run") -BrowsersPath $BrowsersPath
    if ($LASTEXITCODE -ne 0) {
        throw "Playwright dry-run failed with exit code $LASTEXITCODE.`n$output"
    }

    $matches = [regex]::Matches($output, "(?m)^\s*Install location:\s+(.+?)\s*$")
    if ($matches.Count -eq 0) {
        throw "Playwright dry-run did not report any install locations.`n$output"
    }

    $locations = @()
    foreach ($match in $matches) {
        $locations += $match.Groups[1].Value.Trim()
    }

    return $locations
}

function Test-PlaywrightChromiumInstalled {
    param(
        [string]$PlaywrightCommand,
        [string]$BrowsersPath
    )

    $locations = Get-PlaywrightInstallLocations -PlaywrightCommand $PlaywrightCommand -BrowsersPath $BrowsersPath
    foreach ($location in $locations) {
        if (-not (Test-Path $location)) {
            return $false
        }
    }

    return $true
}

function Ensure-PlaywrightChromium {
    param(
        [string]$SourceRoot,
        [string]$BundleRoot
    )

    if ($SkipPlaywrightSetup) {
        Write-Step "Skipping Playwright browser setup for this install run"
        return
    }

    if (-not (Test-PlaywrightPackagePresent -SourceRoot $SourceRoot)) {
        return
    }

    $nodeRuntime = Ensure-NodeRuntime -BundleRoot $BundleRoot
    if ($nodeRuntime.Managed) {
        Write-Step "Using managed Node.js runtime for package dependency setup"
    }

    Ensure-PlaywrightDependencies -SourceRoot $SourceRoot -NpmPath $nodeRuntime.Npm

    $playwrightCommand = Get-LocalPlaywrightCommand -SourceRoot $SourceRoot
    if (-not $playwrightCommand) {
        throw "Playwright CLI was not found after npm dependency installation."
    }

    $browsersPath = Get-ManagedPlaywrightRoot
    Ensure-InstallDir -TargetDir $browsersPath

    if (Test-PlaywrightChromiumInstalled -PlaywrightCommand $playwrightCommand -BrowsersPath $browsersPath) {
        Write-Step "Playwright Chromium is already installed; leaving the existing browser bundle in place"
        return
    }

    $bundledPlaywrightRoot = $null
    if (-not [string]::IsNullOrWhiteSpace($BundleRoot)) {
        $bundledPlaywrightRoot = Get-BundledPlaywrightRoot -Root $BundleRoot
    }
    if ($bundledPlaywrightRoot) {
        Write-Step "Using bundled Playwright browser bundle"
        Copy-DirectoryContents -Source $bundledPlaywrightRoot -Destination $browsersPath
        if (Test-PlaywrightChromiumInstalled -PlaywrightCommand $playwrightCommand -BrowsersPath $browsersPath) {
            Write-Step "Playwright Chromium is available from the bundled browser payload"
            return
        }
    }

    Write-Step "Installing Playwright Chromium browser bundle"
    Push-Location $SourceRoot
    try {
        Invoke-Playwright -PlaywrightCommand $playwrightCommand -Arguments @("install", "chromium") -BrowsersPath $browsersPath | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "Playwright Chromium install failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
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
    Get-CimInstance Win32_Process -Filter "Name = 'nuclear.exe' OR Name = 'autism.exe'" -ErrorAction SilentlyContinue |
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

    Write-Step "Stopping any running Nuclear Agent compatibility processes from the install directory"
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

function Get-AppConfigPath {
    if ([string]::IsNullOrWhiteSpace($env:APPDATA)) {
        return $null
    }

    $configPath = Join-Path $env:APPDATA "NuclearAI\Agent Builder\config\config.json"
    if (Test-Path $configPath) {
        return $configPath
    }

    return $null
}

function Get-ConfiguredDaemonEndpoint {
    $configPath = Get-AppConfigPath
    if (-not $configPath) {
        return $null
    }

    try {
        $config = Get-Content -Path $configPath -Raw | ConvertFrom-Json
    } catch {
        return $null
    }

    if (-not $config.daemon) {
        return $null
    }

    if ([string]::IsNullOrWhiteSpace([string]$config.daemon.host) -or -not $config.daemon.port) {
        return $null
    }

    return @{
        Host = [string]$config.daemon.host
        Port = [int]$config.daemon.port
        Token = [string]$config.daemon.token
    }
}

function Get-ListeningProcessIdsForPort {
    param([int]$Port)

    $processIds = @()

    try {
        $connections = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction Stop)
        foreach ($connection in $connections) {
            if ($connection.OwningProcess) {
                $processIds += [int]$connection.OwningProcess
            }
        }
    } catch {
    }

    if ($processIds.Count -eq 0) {
        try {
            $netstatOutput = & netstat -ano -p tcp 2>$null
            foreach ($line in $netstatOutput) {
                if ($line -match "^\s*TCP\s+\S+:$Port\s+\S+\s+LISTENING\s+(\d+)\s*$") {
                    $processIds += [int]$matches[1]
                }
            }
        } catch {
        }
    }

    return @($processIds | Select-Object -Unique)
}

function Test-PortListening {
    param([int]$Port)

    return @(Get-ListeningProcessIdsForPort -Port $Port).Length -gt 0
}

function Wait-ForPortToClose {
    param(
        [int]$Port,
        [int]$TimeoutSeconds = 15
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (-not (Test-PortListening -Port $Port)) {
            return $true
        }
        Start-Sleep -Milliseconds 500
    }

    return (-not (Test-PortListening -Port $Port))
}

function Stop-ConfiguredDaemon {
    param([string]$BinaryPath)

    $daemon = Get-ConfiguredDaemonEndpoint
    if (-not $daemon) {
        return
    }

    if (-not (Test-PortListening -Port $daemon.Port)) {
        return
    }

    Write-Step "Stopping configured daemon on $($daemon.Host):$($daemon.Port)"

    $stopRequested = $false
    if (Test-Path $BinaryPath) {
        try {
            & $BinaryPath daemon stop *> $null
            if ($LASTEXITCODE -eq 0) {
                $stopRequested = $true
            }
        } catch {
        }
    }

    if (-not $stopRequested -and -not [string]::IsNullOrWhiteSpace($daemon.Token)) {
        try {
            Invoke-RestMethod `
                -Method Post `
                -Uri ("http://{0}:{1}/v1/shutdown" -f $daemon.Host, $daemon.Port) `
                -Headers @{ Authorization = "Bearer $($daemon.Token)" } `
                -ContentType "application/json" `
                -Body "{}" | Out-Null
            $stopRequested = $true
        } catch {
        }
    }

    if (Wait-ForPortToClose -Port $daemon.Port -TimeoutSeconds 15) {
        return
    }

    $processIds = @(Get-ListeningProcessIdsForPort -Port $daemon.Port)
    foreach ($processId in $processIds) {
        try {
            $process = Get-Process -Id $processId -ErrorAction Stop
        } catch {
            continue
        }

        if ($process.ProcessName -in @("nuclear", "autism")) {
            Write-Step "Forcing shutdown of lingering Nuclear Agent daemon on port $($daemon.Port)"
            try {
                Stop-Process -Id $processId -Force -ErrorAction Stop
            } catch {
            }
        }
    }

    [void](Wait-ForPortToClose -Port $daemon.Port -TimeoutSeconds 5)
}

function Build-BinaryFromSource {
    param(
        [string]$SourceRoot,
        [string]$TargetBinary,
        [string]$LegacyBinary
    )

    $cargoPath = Ensure-Cargo

    Write-Step "Building Nuclear Agent from source"
    Push-Location $SourceRoot
    try {
        & $cargoPath build --release --bin nuclear --bin autism
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed"
        }
    } finally {
        Pop-Location
    }

    $builtBinary = Join-Path $SourceRoot "target\release\nuclear.exe"
    if (-not (Test-Path $builtBinary)) {
        throw "Cargo reported success but $builtBinary was not found."
    }

    $builtLegacyBinary = Join-Path $SourceRoot "target\release\autism.exe"
    if (-not (Test-Path $builtLegacyBinary)) {
        throw "Cargo reported success but $builtLegacyBinary was not found."
    }

    Copy-FileAtomic -Source $builtBinary -Destination $TargetBinary
    Copy-FileAtomic -Source $builtLegacyBinary -Destination $LegacyBinary
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
        $InstallDir = if (-not [string]::IsNullOrWhiteSpace($env:NUCLEAR_INSTALL_DIR)) {
            $env:NUCLEAR_INSTALL_DIR
        } elseif (-not [string]::IsNullOrWhiteSpace($env:AUTISM_INSTALL_DIR)) {
            $env:AUTISM_INSTALL_DIR
        } else {
            $null
        }
    }

    if ([string]::IsNullOrWhiteSpace($InstallDir)) {
        $InstallDir = Choose-DefaultInstallDir
    }

    $binaryPath = Join-Path $InstallDir "nuclear.exe"
    $legacyBinaryPath = Join-Path $InstallDir "autism.exe"
    $bundledBinary = Resolve-BundledBinary -Root $scriptRoot

    Write-Step "Installing Nuclear Agent CLI"
    Write-Step "Install directory: $InstallDir"
    Ensure-InstallDir -TargetDir $InstallDir
    Stop-InstalledAutismProcesses -BinaryPath $binaryPath
    Stop-InstalledAutismProcesses -BinaryPath $legacyBinaryPath

    $usedBundledBinary = $false
    if ($bundledBinary -and -not $PreferSourceBuild) {
        Write-Step "Using bundled Windows binary"
        Copy-FileAtomic -Source $bundledBinary -Destination $binaryPath
        Copy-FileAtomic -Source $bundledBinary -Destination $legacyBinaryPath
        Unblock-PathTree -Path $binaryPath
        Unblock-PathTree -Path $legacyBinaryPath
        $usedBundledBinary = $true
    } else {
        Build-BinaryFromSource -SourceRoot $sourceRoot -TargetBinary $binaryPath -LegacyBinary $legacyBinaryPath
        Unblock-PathTree -Path $binaryPath
        Unblock-PathTree -Path $legacyBinaryPath
    }

    Remove-Item -Path (Join-Path $InstallDir "nuclear.cmd") -Force -ErrorAction SilentlyContinue
    Remove-Item -Path (Join-Path $InstallDir "autism.cmd") -Force -ErrorAction SilentlyContinue
    Stop-ConfiguredDaemon -BinaryPath $binaryPath

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
            Build-BinaryFromSource -SourceRoot $sourceRoot -TargetBinary $binaryPath -LegacyBinary $legacyBinaryPath
            Unblock-PathTree -Path $binaryPath
            Unblock-PathTree -Path $legacyBinaryPath
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
    Ensure-PlaywrightChromium -SourceRoot $sourceRoot -BundleRoot $scriptRoot
    Write-Step "Run: nuclear"
    Write-Step "Legacy command compatibility: autism"
    Write-Step "If an already-open terminal does not recognize the command, close it and open a new terminal window."
} catch {
    $root = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $PSCommandPath }
    $logPath = Write-InstallerErrorLog -ErrorRecord $_ -Root $root
    Write-Error "Installation failed. See $logPath for details."
    exit 1
}
