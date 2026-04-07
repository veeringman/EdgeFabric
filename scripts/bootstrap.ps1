# EdgeFabric — Bootstrap Installer (Windows)
#
# Run directly from the internet:
#   irm https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.ps1 | iex
#
# Or with flags:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.ps1))) -AgentOnly
#
# What it does:
#   1. Ensures git is installed
#   2. Clones the EdgeFabric repo (or pulls if it already exists)
#   3. Runs scripts\install.ps1 with any flags you passed
#
param(
    [switch]$AgentOnly,
    [switch]$SkipDocker,
    [switch]$SkipRust,
    [switch]$SkipAndroid
)

$ErrorActionPreference = "Stop"

function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Has($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }

$RepoUrl = "https://github.com/veeringman/EdgeFabric.git"
$InstallDir = if ($env:EDGEFABRIC_DIR) { $env:EDGEFABRIC_DIR } else { Join-Path $HOME "edgefabric" }
$Branch = if ($env:EDGEFABRIC_BRANCH) { $env:EDGEFABRIC_BRANCH } else { "main" }

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "       EdgeFabric Bootstrap Installer" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# ── 1. Ensure git ────────────────────────────────────────────────
if (-not (Has "git")) {
    Write-Info "Git not found - installing..."
    $UseWinget = Has "winget"
    $UseChoco = Has "choco"
    if ($UseWinget) {
        winget install --id Git.Git --accept-package-agreements --accept-source-agreements --silent
    } elseif ($UseChoco) {
        choco install git -y
    } else {
        Write-Err "Cannot install git automatically. Please install git and re-run."
        Write-Err "  https://git-scm.com/download/win"
        exit 1
    }
    # Refresh PATH
    $env:PATH = [System.Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("PATH", "User")
}
Write-Ok "git $(git --version)"

# ── 2. Clone or update repo ─────────────────────────────────────
if (Test-Path (Join-Path $InstallDir ".git")) {
    Write-Info "EdgeFabric repo found at $InstallDir - pulling latest..."
    git -C $InstallDir fetch origin $Branch
    git -C $InstallDir checkout $Branch
    git -C $InstallDir pull --ff-only origin $Branch
    Write-Ok "Repository updated"
} else {
    Write-Info "Cloning EdgeFabric to $InstallDir ..."
    git clone --branch $Branch $RepoUrl $InstallDir
    Write-Ok "Repository cloned"
}

# ── 3. Run the full install script ───────────────────────────────
Write-Info "Running install script..."
Write-Host ""
$installArgs = @()
if ($AgentOnly)   { $installArgs += "-AgentOnly" }
if ($SkipDocker)  { $installArgs += "-SkipDocker" }
if ($SkipRust)    { $installArgs += "-SkipRust" }
if ($SkipAndroid) { $installArgs += "-SkipAndroid" }

# Bypass execution policy — use -EncodedCommand to run the script in-memory
$installScript = Join-Path $InstallDir "scripts\install.ps1"
$cmd = "& '$installScript' $($installArgs -join ' ')"
$bytes = [System.Text.Encoding]::Unicode.GetBytes($cmd)
$encoded = [Convert]::ToBase64String($bytes)
powershell.exe -ExecutionPolicy Bypass -NoProfile -EncodedCommand $encoded
