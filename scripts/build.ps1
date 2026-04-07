# EdgeFabric -- Build Script (Windows)
#
# Usage:
#   .\scripts\build.ps1                    # build all (debug)
#   .\scripts\build.ps1 -Release           # build all (release, optimised)
#   .\scripts\build.ps1 api               # build only ef-api
#   .\scripts\build.ps1 agent             # build only ef-agent
#   .\scripts\build.ps1 cli              # build only ef (CLI)
#   .\scripts\build.ps1 -Release api agent  # release build specific targets
#
param(
    [switch]$Release,
    [switch]$Help,
    [Parameter(ValueFromRemainingArguments)]
    [string[]]$Targets
)

$ErrorActionPreference = "Stop"

function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }

if ($Help) {
    Write-Host "Usage: .\scripts\build.ps1 [-Release] [api|agent|cli|all]"
    exit 0
}

# Navigate to project root
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
Push-Location $ProjectRoot

try {
    # Defaults
    if (-not $Targets -or $Targets.Count -eq 0) { $Targets = @("all") }

    $CargoFlags = @()
    if ($Release) {
        $CargoFlags += "--release"
        $BuildDir = "target\release"
        Write-Info "Building in RELEASE mode"
    } else {
        $BuildDir = "target\debug"
        Write-Info "Building in DEBUG mode"
    }

    # Pre-flight
    if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
        Write-Err "Rust/cargo not found. Run: .\scripts\install.ps1"
        exit 1
    }
    if (-not (Get-Command "protoc" -ErrorAction SilentlyContinue)) {
        Write-Err "protoc not found. Run: .\scripts\install.ps1"
        exit 1
    }

    # Ensure MSVC link.exe is on PATH (needed by Rust on Windows)
    if (-not (Get-Command "link.exe" -ErrorAction SilentlyContinue)) {
        Write-Info "link.exe not found -- loading VS Developer environment..."
        $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
        $vsLoaded = $false
        if (Test-Path $vsWhere) {
            $vsPath = & $vsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
            if (-not $vsPath) {
                # Broader search: any VS/Build Tools installation
                $vsPath = & $vsWhere -latest -products * -property installationPath 2>$null
            }
            if ($vsPath) {
                Write-Info "Found VS at: $vsPath"
                # Try vcvars64.bat first (most reliable across VS editions)
                $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
                if (Test-Path $vcvars) {
                    Write-Info "Loading environment via vcvars64.bat..."
                    $envOutput = cmd /c "`"$vcvars`" >nul 2>&1 && set"
                    foreach ($line in $envOutput) {
                        if ($line -match "^(.+?)=(.*)$") {
                            [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
                        }
                    }
                    $vsLoaded = $true
                    Write-Ok "VS environment loaded via vcvars64.bat"
                } else {
                    # Fallback: Launch-VsDevShell.ps1
                    $devShell = Join-Path $vsPath "Common7\Tools\Launch-VsDevShell.ps1"
                    if (Test-Path $devShell) {
                        Write-Info "Loading environment via Launch-VsDevShell.ps1..."
                        try {
                            & $devShell -SkipAutomaticLocation | Out-Null
                            $vsLoaded = $true
                            Write-Ok "VS Developer environment loaded"
                        } catch {
                            Write-Err "Launch-VsDevShell.ps1 failed: $_"
                        }
                    } else {
                        Write-Err "Neither vcvars64.bat nor Launch-VsDevShell.ps1 found under $vsPath"
                    }
                }
            } else {
                Write-Err "vswhere found but no VS installation detected."
                Write-Err "Install 'Desktop development with C++' workload:"
                Write-Err "  winget install Microsoft.VisualStudio.2022.BuildTools --override ""--add Microsoft.VisualStudio.Workload.VCTools --passive"""
            }
        } else {
            Write-Err "vswhere.exe not found at: $vsWhere"
            Write-Err "Install Visual Studio Build Tools:"
            Write-Err "  winget install Microsoft.VisualStudio.2022.BuildTools --override ""--add Microsoft.VisualStudio.Workload.VCTools --passive"""
        }
        if (-not (Get-Command "link.exe" -ErrorAction SilentlyContinue)) {
            if ($vsLoaded) {
                Write-Err "VS environment loaded but link.exe still not on PATH."
                Write-Err "The C++ build tools may not be installed. Add the 'Desktop development with C++' workload."
            } else {
                Write-Err "link.exe not found. Open 'Developer PowerShell for VS 2022' or install C++ build tools:"
                Write-Err "  winget install Microsoft.VisualStudio.2022.BuildTools --override ""--add Microsoft.VisualStudio.Workload.VCTools --passive"""
            }
            exit 1
        }
    }

    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    foreach ($target in $Targets) {
        switch ($target) {
            "all" {
                Write-Info "Building all workspace crates..."
                cargo build @CargoFlags
                if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
                Write-Ok "All crates built -> $BuildDir\"
            }
            "api" {
                Write-Info "Building ef-api..."
                cargo build -p ef-api @CargoFlags
                if ($LASTEXITCODE -ne 0) { throw "cargo build ef-api failed" }
                Write-Ok "ef-api built -> $BuildDir\"
            }
            "agent" {
                Write-Info "Building ef-agent..."
                cargo build -p ef-agent @CargoFlags
                if ($LASTEXITCODE -ne 0) { throw "cargo build ef-agent failed" }
                Write-Ok "ef-agent built -> $BuildDir\"
            }
            "cli" {
                Write-Info "Building ef-cli..."
                cargo build -p ef-cli @CargoFlags
                if ($LASTEXITCODE -ne 0) { throw "cargo build ef-cli failed" }
                Write-Ok "ef-cli built -> $BuildDir\"
            }
            default {
                Write-Err "Unknown target: $target (use: all|api|agent|cli)"
                exit 1
            }
        }
    }

    $sw.Stop()

    Write-Host ""
    Write-Host "================================================" -ForegroundColor Green
    Write-Host " Build completed in $([math]::Round($sw.Elapsed.TotalSeconds))s" -ForegroundColor Green
    Write-Host "================================================" -ForegroundColor Green

    if ($Release) {
        Write-Host ""
        Write-Host "  Binaries:"
        foreach ($bin in @("ef-api.exe", "ef-agent.exe", "ef.exe")) {
            $path = Join-Path $BuildDir $bin
            if (Test-Path $path) {
                $size = [math]::Round((Get-Item $path).Length / 1048576, 1)
                Write-Host ('    ' + $path + '  ' + $size + ' MB')
            }
        }
    }
    Write-Host ""
} finally {
    Pop-Location
}
