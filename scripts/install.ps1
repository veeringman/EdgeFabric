# EdgeFabric — Install Prerequisites (Windows)
# Requires: PowerShell 5.1+ (run as Administrator for Chocolatey/winget installs)
#
# Usage: .\scripts\install.ps1 [-AgentOnly] [-SkipDocker] [-SkipRust] [-SkipAndroid]
#
param(
    [switch]$AgentOnly,
    [switch]$SkipDocker,
    [switch]$SkipRust,
    [switch]$SkipAndroid,
    [switch]$Help
)

# ── Helpers ───────────────────────────────────────────────────────
function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Has($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }

function Pkg-Install($wingetId, $chocoId) {
    if (Has "winget") {
        Write-Info "Installing via winget: $wingetId"
        winget install --id $wingetId --accept-package-agreements --accept-source-agreements --silent
    } elseif (Has "choco") {
        Write-Info "Installing via choco: $chocoId"
        choco install $chocoId -y
    } else {
        Write-Warn "No package manager available to install $chocoId"
    }
}

function Refresh-Path {
    $env:PATH = [System.Environment]::GetEnvironmentVariable("PATH","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("PATH","User")
}

if ($Help) {
    Write-Host "Usage: .\scripts\install.ps1 [-AgentOnly] [-SkipDocker] [-SkipRust] [-SkipAndroid]"
    Write-Host ""
    Write-Host "  -AgentOnly   Install only what's needed to run the edge agent"
    Write-Host "               (Rust, protoc, device tools - skips Docker/infra)"
    exit 0
}

if ($AgentOnly) {
    $SkipDocker = $true
    Write-Info "Agent-only mode: skipping Docker/infrastructure dependencies"
}

Write-Host ""
Write-Host "EdgeFabric - Windows Prerequisites Installer" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host ""

# ── 0. Ensure a package manager ──────────────────────────────────
if (-not (Has "winget") -and -not (Has "choco")) {
    Write-Warn "Neither winget nor Chocolatey found."
    Write-Info "Installing Chocolatey..."
    try {
        Set-ExecutionPolicy Bypass -Scope Process -Force
        [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
        Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
        Refresh-Path
        Write-Ok "Chocolatey installed"
    } catch {
        Write-Err "Failed to install Chocolatey: $_"
    }
}

# ── 1. Git ────────────────────────────────────────────────────────
Write-Info "Checking Git..."
if (-not (Has "git")) {
    Pkg-Install "Git.Git" "git"
    Refresh-Path
}
if (Has "git") { Write-Ok "Git $(git --version)" }

# ── 2. Visual Studio Build Tools (C/C++ linker for Rust) ─────────
Write-Info "Checking C/C++ build tools..."
$vsInstalled = $false
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vsWhere) {
    $vsPath = & $vsWhere -latest -property installationPath 2>$null
    if ($vsPath) {
        Write-Ok "Visual Studio / Build Tools found: $vsPath"
        $vsInstalled = $true
    }
}
if (-not $vsInstalled) {
    Write-Info "Installing Visual Studio Build Tools (C++ workload)..."
    Write-Info "This may take several minutes..."
    try {
        $vsUrl = "https://aka.ms/vs/17/release/vs_BuildTools.exe"
        $vsInstaller = "$env:TEMP\vs_BuildTools.exe"
        Invoke-WebRequest -Uri $vsUrl -OutFile $vsInstaller -UseBasicParsing
        Start-Process -FilePath $vsInstaller -ArgumentList @(
            "--quiet", "--wait", "--norestart",
            "--add", "Microsoft.VisualStudio.Workload.VCTools",
            "--includeRecommended"
        ) -Wait -NoNewWindow
        Remove-Item $vsInstaller -ErrorAction SilentlyContinue
        Write-Ok "Visual Studio Build Tools installed"
        Write-Warn "You may need to RESTART your terminal for link.exe to be on PATH"
    } catch {
        Write-Err "VS Build Tools install failed: $_"
        Write-Warn "Install manually: https://visualstudio.microsoft.com/visual-cpp-build-tools/"
    }
}

# ── 3. Rust toolchain ────────────────────────────────────────────
if (-not $SkipRust) {
    Write-Info "Checking Rust toolchain..."
    if (-not (Has "rustup")) {
        Write-Info "Installing Rust via rustup..."
        try {
            $rustupInit = "$env:TEMP\rustup-init.exe"
            Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit -UseBasicParsing
            & $rustupInit -y --default-toolchain stable
            $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
        } catch {
            Write-Err "Rust install failed: $_"
        }
    }
    if (Has "rustc") { Write-Ok "Rust: $(rustc --version)" }

    if (-not (Has "sqlx") -and -not $AgentOnly) {
        Write-Info "Installing sqlx-cli..."
        try {
            cargo install sqlx-cli --no-default-features --features postgres 2>&1 | Out-Null
            Write-Ok "sqlx-cli installed"
        } catch {
            Write-Warn "sqlx-cli install failed (may need link.exe). Retry after restarting terminal."
        }
    }
    Write-Ok "Rust toolchain ready"
} else {
    Write-Warn "Skipping Rust toolchain (-SkipRust)"
}

# ── 4. Protobuf compiler ─────────────────────────────────────────
Write-Info "Checking protobuf compiler..."
if (-not (Has "protoc")) {
    Pkg-Install "Google.Protobuf" "protoc"
    Refresh-Path
}
if (Has "protoc") { Write-Ok "protoc $(protoc --version)" }

# ── 5. Docker Desktop ────────────────────────────────────────────
if (-not $SkipDocker) {
    Write-Info "Checking Docker..."
    if (-not (Has "docker")) {
        Pkg-Install "Docker.DockerDesktop" "docker-desktop"
        Refresh-Path
    }
    if (Has "docker") { Write-Ok "Docker $(docker --version)" }
} else {
    Write-Warn "Skipping Docker (-SkipDocker)"
}

# ── 6. Android tools ─────────────────────────────────────────────
if (-not $SkipAndroid) {
    Write-Info "Checking Android tools..."
    if (-not (Has "adb")) {
        Pkg-Install "Google.PlatformTools" "adb"
        Refresh-Path
    }
    if (Has "adb") { Write-Ok "adb available" }
    else { Write-Warn "ADB not found. Install Android SDK Platform Tools manually." }
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
    Write-Host '    2. $env:EF__AGENT__CONTROL_PLANE_URL="http://<api-server>:6060"'
    Write-Host '       .\scripts\deploy.ps1 agent                   # start the agent'
} else {
    Write-Host "    1. .\scripts\build.ps1          # build EdgeFabric binaries"
    Write-Host "    2. .\scripts\deploy.ps1 infra   # start Postgres, Redis, NATS"
    Write-Host "    3. .\scripts\deploy.ps1 api     # start the API server"
    Write-Host "    4. .\scripts\deploy.ps1 agent   # start the agent"
}
Write-Host ""
