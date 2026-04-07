# EdgeFabric -- Start Android Emulator (Windows)
#
# Usage:
#   .\scripts\start-emulator.ps1                    # list AVDs and start the first one
#   .\scripts\start-emulator.ps1 -Name Pixel_6      # start a specific AVD
#   .\scripts\start-emulator.ps1 -Create             # create a default AVD if none exist
#   .\scripts\start-emulator.ps1 -List               # list available AVDs
#   .\scripts\start-emulator.ps1 -Wipe               # cold boot (wipe user data)
#   .\scripts\start-emulator.ps1 -Headless            # no GUI window (CI/testing)
#   .\scripts\start-emulator.ps1 -GpuMode swiftshader # software rendering (no GPU)
#
param(
    [string]$Name,
    [switch]$Create,
    [switch]$List,
    [switch]$Wipe,
    [switch]$Headless,
    [string]$GpuMode = "auto",
    [switch]$Help
)

$ErrorActionPreference = "Stop"

function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Has($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }

if ($Help) {
    Get-Content $MyInvocation.MyCommand.Path | Select-String "^#" | Select-Object -First 11 | ForEach-Object { $_ -replace "^# ?","" }
    exit 0
}

Write-Host ""
Write-Host "EdgeFabric -- Android Emulator Launcher" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ── Locate Android SDK ───────────────────────────────────────────
$SdkRoot = $null
foreach ($candidate in @(
    $env:ANDROID_HOME,
    $env:ANDROID_SDK_ROOT,
    "$env:LOCALAPPDATA\Android\Sdk",
    "$env:USERPROFILE\AppData\Local\Android\Sdk",
    "C:\Android\Sdk"
)) {
    if ($candidate -and (Test-Path "$candidate\emulator")) {
        $SdkRoot = $candidate
        break
    }
}

if (-not $SdkRoot) {
    Write-Err "Android SDK not found."
    Write-Host ""
    Write-Host "Install options:" -ForegroundColor Yellow
    Write-Host "  1. Android Studio: https://developer.android.com/studio"
    Write-Host "  2. Command-line tools only:"
    Write-Host "     winget install Google.AndroidCommandLineTools"
    Write-Host ""
    Write-Host "Then set ANDROID_HOME to the SDK path."
    exit 1
}

Write-Info "SDK root: $SdkRoot"

$Emulator   = Join-Path $SdkRoot "emulator\emulator.exe"
$Avdmanager = Join-Path $SdkRoot "cmdline-tools\latest\bin\avdmanager.bat"
$Sdkmanager = Join-Path $SdkRoot "cmdline-tools\latest\bin\sdkmanager.bat"
$Adb        = Join-Path $SdkRoot "platform-tools\adb.exe"

# Fallback paths for cmdline-tools
if (-not (Test-Path $Avdmanager)) {
    $altTools = Get-ChildItem -Path "$SdkRoot\cmdline-tools" -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -First 1
    if ($altTools) {
        $Avdmanager = Join-Path $altTools.FullName "bin\avdmanager.bat"
        $Sdkmanager = Join-Path $altTools.FullName "bin\sdkmanager.bat"
    }
}

if (-not (Test-Path $Emulator)) {
    Write-Err "emulator.exe not found at: $Emulator"
    Write-Host "Run: sdkmanager 'emulator' 'platform-tools'"
    exit 1
}

# ── List AVDs ─────────────────────────────────────────────────────
function Get-Avds {
    $output = & $Emulator -list-avds 2>$null
    return ($output | Where-Object { $_ -match '\S' })
}

$avds = Get-Avds

if ($List) {
    Write-Host "Available AVDs:" -ForegroundColor Cyan
    if ($avds) {
        $avds | ForEach-Object { Write-Host "  - $_" }
    } else {
        Write-Host "  (none found)"
        Write-Host ""
        Write-Host "Create one with: .\scripts\start-emulator.ps1 -Create"
    }
    exit 0
}

# ── Create default AVD if requested or none exist ─────────────────
if ($Create -or (-not $avds)) {
    if (-not $avds) {
        Write-Warn "No AVDs found."
    }
    Write-Info "Creating a default AVD..."

    if (-not (Test-Path $Sdkmanager)) {
        Write-Err "sdkmanager not found. Install Android command-line tools first."
        exit 1
    }

    # Install system image and platform
    $SysImage = "system-images;android-34;google_apis;x86_64"
    Write-Info "Downloading system image (API 34, Google APIs, x86_64)..."
    Write-Info "This may take a few minutes on first run..."
    echo "y" | & $Sdkmanager --install $SysImage "platforms;android-34" "emulator" "platform-tools" 2>&1 | Out-Null

    # Create AVD
    $DefaultAvd = "EdgeFabric_Pixel6"
    if (-not (Test-Path $Avdmanager)) {
        Write-Err "avdmanager not found at: $Avdmanager"
        exit 1
    }

    Write-Info "Creating AVD: $DefaultAvd (Pixel 6, API 34)..."
    echo "no" | & $Avdmanager create avd `
        --name $DefaultAvd `
        --package $SysImage `
        --device "pixel_6" `
        --force 2>&1 | Out-Null

    Write-Ok "AVD '$DefaultAvd' created"
    $Name = $DefaultAvd
    $avds = Get-Avds
}

# ── Select AVD ────────────────────────────────────────────────────
if (-not $Name) {
    $Name = $avds | Select-Object -First 1
    Write-Info "Using AVD: $Name"
}

if (-not ($avds -contains $Name)) {
    Write-Err "AVD '$Name' not found."
    Write-Host "Available AVDs:"
    $avds | ForEach-Object { Write-Host "  - $_" }
    exit 1
}

# ── Build emulator flags ─────────────────────────────────────────
$emuFlags = @("-avd", $Name, "-gpu", $GpuMode)

if ($Wipe) {
    $emuFlags += "-wipe-data"
    Write-Info "Cold boot (wiping user data)"
}

if ($Headless) {
    $emuFlags += @("-no-window", "-no-audio", "-no-boot-anim")
    Write-Info "Headless mode (no GUI)"
}

# Enable ADB on all interfaces so the control plane can reach it
$emuFlags += @("-dns-server", "8.8.8.8")

Write-Info "Starting emulator: $Name"
Write-Host "  Flags: $($emuFlags -join ' ')" -ForegroundColor DarkGray

# ── Launch ────────────────────────────────────────────────────────
$proc = Start-Process -FilePath $Emulator -ArgumentList $emuFlags -PassThru

Write-Host ""
Write-Info "Emulator PID: $($proc.Id)"
Write-Info "Waiting for device to boot..."

# Wait for adb device
if (Test-Path $Adb) {
    $booted = $false
    for ($i = 0; $i -lt 120; $i++) {
        $bootAnim = & $Adb shell getprop sys.boot_completed 2>$null
        if ($bootAnim -match "1") {
            $booted = $true
            break
        }
        Start-Sleep -Seconds 2
    }

    if ($booted) {
        Write-Ok "Emulator booted and ready!"
        Write-Host ""

        # Show device info
        $model  = (& $Adb shell getprop ro.product.model 2>$null).Trim()
        $api    = (& $Adb shell getprop ro.build.version.sdk 2>$null).Trim()
        $serial = (& $Adb get-serialno 2>$null).Trim()

        Write-Host "  Device:  $model" -ForegroundColor Green
        Write-Host "  API:     $api" -ForegroundColor Green
        Write-Host "  Serial:  $serial" -ForegroundColor Green
        Write-Host ""
        Write-Host "  The ef-agent will auto-discover this device." -ForegroundColor Cyan
        Write-Host "  Start the agent: .\scripts\deploy.ps1 agent" -ForegroundColor Cyan
    } else {
        Write-Warn "Emulator started but boot timed out (120s)."
        Write-Host "  It may still be loading. Check: adb devices"
    }
} else {
    Write-Warn "adb not found -- cannot detect boot completion."
    Write-Host "  The emulator window should appear shortly."
}

Write-Host ""
