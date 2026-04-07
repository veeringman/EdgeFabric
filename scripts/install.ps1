# EdgeFabric — Install Prerequisites (Windows)
# Requires: PowerShell 5.1+ (run as Administrator for Chocolatey/winget installs)
#
# Usage: .\scripts\install.ps1 [-SkipDocker] [-SkipRust] [-SkipAndroid]
#
param(
    [switch]$AgentOnly,
    [switch]$SkipDocker,
    [switch]$SkipRust,
    [switch]$SkipAndroid,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

# ── Helpers ───────────────────────────────────────────────────────
function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Has($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }

if ($Help) {
    Write-Host "Usage: .\scripts\install.ps1 [-AgentOnly] [-SkipDocker] [-SkipRust] [-SkipAndroid]"
    Write-Host ""
    Write-Host "  -AgentOnly   Install only what's needed to run the edge agent"
    Write-Host "               (Rust, protoc, device tools - skips Docker/infra)"
    exit 0
}

# Agent-only mode: skip Docker (infra runs on a remote server)
if ($AgentOnly) {
    $SkipDocker = $true
    Write-Info "Agent-only mode: skipping Docker/infrastructure dependencies"
}

Write-Host ""
Write-Host "EdgeFabric — Windows Prerequisites Installer" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host ""

# ── Detect package manager ────────────────────────────────────────
$UseWinget = Has "winget"
$UseChoco  = Has "choco"

if (-not $UseWinget -and -not $UseChoco) {
    Write-Warn "Neither winget nor Chocolatey found."
    Write-Info "Installing Chocolatey..."
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
    $UseChoco = $true
}

function Install-Pkg($wingetId, $chocoId) {
    if ($UseWinget) {
        winget install --id $wingetId --accept-package-agreements --accept-source-agreements --silent
    } elseif ($UseChoco) {
        choco install $chocoId -y
    }
}

# ── 1. Git ────────────────────────────────────────────────────────
Write-Info "Checking Git..."
if (-not (Has "git")) {
    Install-Pkg "Git.Git" "git"
}
Write-Ok "Git $(git --version)"

# ── 2. Visual Studio Build Tools (C/C++ linker for Rust) ─────────
Write-Info "Checking C/C++ build tools..."
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vsWhere) {
    $vsPath = & $vsWhere -latest -property installationPath 2>$null
    if ($vsPath) {
        Write-Ok "Visual Studio / Build Tools found: $vsPath"
    }
} else {
    Write-Warn "Visual Studio Build Tools not found."
    Write-Warn "Install from: https://visualstudio.microsoft.com/visual-cpp-build-tools/"
    Write-Warn 'Select "Desktop development with C++" workload.'
}

# ── 3. Rust toolchain ────────────────────────────────────────────
if (-not $SkipRust) {
    Write-Info "Checking Rust toolchain..."
    if (-not (Has "rustup")) {
        Write-Info "Installing Rust via rustup..."
        $rustupInit = "$env:TEMP\rustup-init.exe"
        Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit
        & $rustupInit -y --default-toolchain stable
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    }
    $rustVer = rustc --version
    Write-Ok "Rust: $rustVer"

    if (-not (Has "sqlx")) {
        Write-Info "Installing sqlx-cli..."
        cargo install sqlx-cli --no-default-features --features postgres
    }
    Write-Ok "Rust toolchain ready"
} else {
    Write-Warn "Skipping Rust toolchain (-SkipRust)"
}

# ── 4. Protobuf compiler ─────────────────────────────────────────
Write-Info "Checking protobuf compiler..."
if (-not (Has "protoc")) {
    Install-Pkg "Google.Protobuf" "protoc"
}
if (Has "protoc") {
    Write-Ok "protoc $(protoc --version)"
}

# ── 5. Docker Desktop ────────────────────────────────────────────
if (-not $SkipDocker) {
    Write-Info "Checking Docker..."
    if (-not (Has "docker")) {
        Write-Warn "Docker Desktop not found."
        Write-Warn "Install from: https://docker.com/products/docker-desktop"
        if ($UseWinget) {
            Write-Info "Attempting winget install..."
            winget install --id Docker.DockerDesktop --accept-package-agreements --accept-source-agreements --silent
        }
    }
    if (Has "docker") {
        Write-Ok "Docker $(docker --version)"
    }
} else {
    Write-Warn "Skipping Docker (-SkipDocker)"
}

# ── 6. Android tools ─────────────────────────────────────────────
if (-not $SkipAndroid) {
    Write-Info "Checking Android tools..."
    if (-not (Has "adb")) {
        Write-Warn "ADB not found. Install Android SDK Platform Tools:"
        Write-Warn "  https://developer.android.com/studio/releases/platform-tools"
        Write-Warn "  Or: choco install adb"
        if ($UseChoco) {
            choco install adb -y
        }
    }
    if (Has "adb") {
        Write-Ok "adb available"
    }
} else {
    Write-Warn "Skipping Android tools (-SkipAndroid)"
}

# ── 7. iOS note ───────────────────────────────────────────────────
Write-Warn "iOS device support is not available on Windows. Use a macOS machine for iOS testing."

# ── Summary ───────────────────────────────────────────────────────
Write-Host ""
Write-Host "================================================" -ForegroundColor Green
Write-Host " EdgeFabric prerequisites installed successfully!" -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Next steps:"
if ($AgentOnly) {
    Write-Host '    1. .\scripts\build.ps1 agent                    # build the agent binary'
    Write-Host '    2. $env:EF__AGENT__CONTROL_PLANE_URL="https://<api-server>:50051"'
    Write-Host '       .\scripts\deploy.ps1 agent                   # start the agent'
} else {
    Write-Host "    1. .\scripts\build.ps1          # build EdgeFabric binaries"
    Write-Host "    2. .\scripts\deploy.ps1 infra   # start Postgres, Redis, NATS"
    Write-Host "    3. .\scripts\deploy.ps1 api     # start the API server"
    Write-Host "    4. .\scripts\deploy.ps1 agent   # start the agent"
}
Write-Host ""
