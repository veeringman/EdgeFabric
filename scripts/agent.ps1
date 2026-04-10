# EdgeFabric Agent — Windows Management Script
#
# Usage:
#   .\scripts\agent.ps1 start          # build & start agent
#   .\scripts\agent.ps1 stop           # stop agent
#   .\scripts\agent.ps1 restart        # stop + start
#   .\scripts\agent.ps1 logs           # tail agent log
#   .\scripts\agent.ps1 status         # check if running
#   .\scripts\agent.ps1 build          # build only
#   .\scripts\agent.ps1 pull           # git pull + build + restart
#
# Environment:
#   $env:EF_RUN_MODE = "development"    # default
#   $env:AGENT_ID = "win-01"           # override agent ID

param(
    [Parameter(Position=0)]
    [ValidateSet("start","stop","restart","logs","status","build","pull","help")]
    [string]$Command = "help"
)

$ErrorActionPreference = "Stop"

# ── Paths ──────────────────────────────────────────────────────────
$ScriptDir   = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$Binary      = Join-Path $ProjectRoot "target\debug\ef-agent.exe"
$LogFile     = Join-Path $env:TEMP "ef-agent.log"
$PidFile     = Join-Path $env:TEMP "ef-agent.pid"
$RunEnv      = if ($env:EF_RUN_MODE) { $env:EF_RUN_MODE } else { "development" }

Set-Location $ProjectRoot

# ── Helpers ────────────────────────────────────────────────────────
function Write-Info  { param($msg) Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok    { param($msg) Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn  { param($msg) Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Err   { param($msg) Write-Host "[ERROR] $msg" -ForegroundColor Red }

function Get-AgentProcess {
    $procs = Get-Process -Name "ef-agent" -ErrorAction SilentlyContinue
    if (-not $procs -and (Test-Path $PidFile)) {
        $savedPid = Get-Content $PidFile -ErrorAction SilentlyContinue
        if ($savedPid) {
            $procs = Get-Process -Id $savedPid -ErrorAction SilentlyContinue
        }
    }
    return $procs
}

function Test-AgentRunning {
    $proc = Get-AgentProcess
    return ($null -ne $proc)
}

# ── Commands ───────────────────────────────────────────────────────
function Invoke-Build {
    # On Windows, cargo can't overwrite a running exe — stop agent first
    if (Test-AgentRunning) {
        Write-Warn "Stopping agent before build (Windows file lock)..."
        Invoke-Stop
        Start-Sleep -Seconds 1
    }

    Write-Info "Building ef-agent..."
    $prevPref = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & cargo build --bin ef-agent 2>&1 | ForEach-Object { Write-Host $_ }
    $buildExit = $LASTEXITCODE
    $ErrorActionPreference = $prevPref
    if ($buildExit -ne 0) {
        Write-Err "Build failed (exit code $buildExit)"
        exit 1
    }
    Write-Ok "Build complete: $Binary"
}

function Invoke-Start {
    if (Test-AgentRunning) {
        Write-Warn "Agent is already running (PID: $((Get-AgentProcess).Id -join ', '))"
        return
    }

    if (-not (Test-Path $Binary)) {
        Write-Info "Binary not found, building first..."
        Invoke-Build
    }

    Write-Info "Starting ef-agent (env=$RunEnv, log=$LogFile)..."

    $env:EF_RUN_MODE = $RunEnv

    # Set agent ID override if provided
    if ($env:AGENT_ID) {
        $env:EF__AGENT__AGENT_ID = $env:AGENT_ID
        Write-Info "Agent ID: $($env:AGENT_ID)"
    }

    # Copy config files next to binary location if needed
    $configDest = Join-Path $ProjectRoot "config"
    if (-not (Test-Path $configDest)) {
        Write-Err "Config directory not found: $configDest"
        exit 1
    }

    # Start as background process
    $proc = Start-Process -FilePath $Binary `
        -WorkingDirectory $ProjectRoot `
        -RedirectStandardOutput $LogFile `
        -RedirectStandardError "$LogFile.err" `
        -WindowStyle Hidden `
        -PassThru

    $proc.Id | Out-File -FilePath $PidFile -Force

    Start-Sleep -Seconds 2

    if (-not $proc.HasExited) {
        Write-Ok "Agent started (PID: $($proc.Id))"
        Write-Info "Log: $LogFile"

        # Show first few log lines
        if (Test-Path $LogFile) {
            $lines = Get-Content $LogFile -Tail 5 -ErrorAction SilentlyContinue
            if ($lines) {
                Write-Host ""
                $lines | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
            }
        }
    } else {
        Write-Err "Agent exited immediately (exit code: $($proc.ExitCode))"
        Write-Err "Check log: $LogFile"
        if (Test-Path "$LogFile.err") {
            Get-Content "$LogFile.err" -Tail 10 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
        }
        exit 1
    }
}

function Invoke-Stop {
    $procs = Get-AgentProcess
    if (-not $procs) {
        Write-Warn "Agent is not running"
        return
    }

    foreach ($p in $procs) {
        Write-Info "Stopping agent (PID: $($p.Id))..."
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    }

    if (Test-Path $PidFile) { Remove-Item $PidFile -Force }

    Write-Ok "Agent stopped"
}

function Invoke-Restart {
    Invoke-Stop
    Start-Sleep -Seconds 1
    Invoke-Start
}

function Invoke-Logs {
    if (-not (Test-Path $LogFile)) {
        Write-Warn "No log file found: $LogFile"
        return
    }

    Write-Info "Tailing $LogFile (Ctrl+C to stop)..."
    Write-Host ""

    # Show last 50 lines then follow
    Get-Content $LogFile -Tail 50 -Wait | ForEach-Object {
        # Color-code log levels
        if ($_ -match "\bERROR\b") {
            Write-Host $_ -ForegroundColor Red
        } elseif ($_ -match "\bWARN\b") {
            Write-Host $_ -ForegroundColor Yellow
        } elseif ($_ -match "\bINFO\b") {
            Write-Host $_ -ForegroundColor Cyan
        } else {
            Write-Host $_ -ForegroundColor Gray
        }
    }
}

function Invoke-Status {
    $procs = Get-AgentProcess
    if ($procs) {
        foreach ($p in $procs) {
            Write-Ok "Agent is running (PID: $($p.Id), CPU: $([math]::Round($p.CPU, 1))s, Mem: $([math]::Round($p.WorkingSet64/1MB, 1))MB)"
        }

        # Check control plane connectivity
        $configFile = Join-Path $ProjectRoot "config\development.toml"
        if (Test-Path $configFile) {
            $url = (Get-Content $configFile | Select-String 'control_plane_url\s*=\s*"(.+)"').Matches.Groups[1].Value
            if ($url) {
                Write-Info "Control plane: $url"
                try {
                    $resp = Invoke-WebRequest -Uri "$url/api/v1/devices" `
                        -Headers @{"X-API-Key"="efdev-key-00000000"} `
                        -TimeoutSec 3 -ErrorAction Stop
                    Write-Ok "API reachable ($($resp.StatusCode))"
                } catch {
                    Write-Warn "API unreachable: $($_.Exception.Message)"
                }
            }
        }

        # Show last 3 log lines
        if (Test-Path $LogFile) {
            Write-Host ""
            Write-Info "Recent log:"
            Get-Content $LogFile -Tail 3 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
        }
    } else {
        Write-Warn "Agent is not running"
    }
}

function Invoke-Pull {
    Write-Info "Pulling latest changes..."
    & git pull 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) {
        Write-Err "git pull failed"
        exit 1
    }
    Invoke-Build
    if (Test-AgentRunning) {
        Write-Info "Restarting agent with new build..."
        Invoke-Restart
    } else {
        Write-Info "Agent not running. Start with: .\scripts\agent.ps1 start"
    }
}

function Show-Help {
    Write-Host @"

  EdgeFabric Agent — Windows Management Script

  Usage: .\scripts\agent.ps1 <command>

  Commands:
    start       Build (if needed) and start the agent
    stop        Stop the running agent
    restart     Stop + start
    logs        Tail agent log with color coding
    status      Show agent process info + API connectivity
    build       Build ef-agent binary only
    pull        git pull + build + restart

  Environment:
    `$env:EF_RUN_MODE = "development"  Config profile (default: development)
    `$env:AGENT_ID = "win-01"         Override agent ID

  Examples:
    .\scripts\agent.ps1 start
    .\scripts\agent.ps1 logs
    `$env:EF_RUN_MODE = "development"; .\scripts\agent.ps1 start

"@
}

# ── Dispatch ───────────────────────────────────────────────────────
switch ($Command) {
    "start"   { Invoke-Start }
    "stop"    { Invoke-Stop }
    "restart" { Invoke-Restart }
    "logs"    { Invoke-Logs }
    "status"  { Invoke-Status }
    "build"   { Invoke-Build }
    "pull"    { Invoke-Pull }
    "help"    { Show-Help }
}
