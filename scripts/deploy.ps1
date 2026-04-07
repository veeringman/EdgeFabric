# EdgeFabric — Deploy / Run Script (Windows)
#
# Usage:
#   .\scripts\deploy.ps1 infra         # start Postgres, Redis, NATS, CoTURN
#   .\scripts\deploy.ps1 api           # start API server (native)
#   .\scripts\deploy.ps1 agent         # start agent (native)
#   .\scripts\deploy.ps1 all           # infra + api + agent
#   .\scripts\deploy.ps1 docker        # full stack via docker compose
#   .\scripts\deploy.ps1 stop          # stop all services
#   .\scripts\deploy.ps1 status        # check health of all services
#
param(
    [Parameter(Position = 0)]
    [ValidateSet("infra", "api", "agent", "all", "docker", "stop", "status", "help")]
    [string]$Command = "help"
)

$ErrorActionPreference = "Stop"

function Write-Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Cyan }
function Write-Ok($msg)    { Write-Host "[OK]    $msg" -ForegroundColor Green }
function Write-Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Write-Err($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Has($cmd) { $null -ne (Get-Command $cmd -ErrorAction SilentlyContinue) }

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
Push-Location $ProjectRoot

$RunMode = if ($env:EF_RUN_MODE) { $env:EF_RUN_MODE } else { "development" }
$LogDir = Join-Path $ProjectRoot "logs"
$PidDir = Join-Path $ProjectRoot ".pids"

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
New-Item -ItemType Directory -Force -Path $PidDir | Out-Null

# ── Determine binary path ────────────────────────────────────────
$BinDir = $null
if (Test-Path "target\release\ef-api.exe") {
    $BinDir = "target\release"
} elseif (Test-Path "target\release\ef-agent.exe") {
    $BinDir = "target\release"
} elseif (Test-Path "target\debug\ef-api.exe") {
    $BinDir = "target\debug"
} elseif (Test-Path "target\debug\ef-agent.exe") {
    $BinDir = "target\debug"
}

# ── Helper functions ──────────────────────────────────────────────
function Wait-ForPort($port, $name, $timeout = 30) {
    Write-Info "Waiting for $name on port $port..."
    for ($i = 0; $i -lt $timeout; $i++) {
        try {
            $tcp = New-Object System.Net.Sockets.TcpClient
            $tcp.Connect("localhost", $port)
            $tcp.Close()
            Write-Ok "$name is ready on port $port"
            return $true
        } catch {
            Start-Sleep -Seconds 1
        }
    }
    Write-Err "$name did not start on port $port within ${timeout}s"
    return $false
}

function Save-Pid($name, $svcPid) {
    Set-Content -Path (Join-Path $PidDir "$name.pid") -Value $svcPid
}

function Get-SavedPid($name) {
    $pidFile = Join-Path $PidDir "$name.pid"
    if (Test-Path $pidFile) { return Get-Content $pidFile }
    return $null
}

function Stop-Service-ByName($name) {
    $svcPid = Get-SavedPid $name
    if ($svcPid) {
        try {
            $proc = Get-Process -Id $svcPid -ErrorAction SilentlyContinue
            if ($proc) {
                Write-Info "Stopping $name (PID $svcPid)..."
                Stop-Process -Id $svcPid -Force
                Write-Ok "$name stopped"
            } else {
                Write-Warn "$name is not running"
            }
        } catch {
            Write-Warn "$name is not running"
        }
        Remove-Item -Force (Join-Path $PidDir "$name.pid") -ErrorAction SilentlyContinue
    } else {
        Write-Warn "$name is not running"
    }
}

# ── Commands ──────────────────────────────────────────────────────

function Deploy-Infra {
    Write-Info "Starting infrastructure services via Docker Compose..."
    if (-not (Has "docker")) {
        Write-Err "Docker not found. Run: .\scripts\install.ps1"
        exit 1
    }
    docker compose up -d postgres redis nats coturn
    Write-Host ""
    Wait-ForPort 5432 "PostgreSQL"
    Wait-ForPort 6379 "Redis"
    Wait-ForPort 4222 "NATS"
    Write-Host ""
    Write-Ok "Infrastructure services running"
}

function Deploy-Api {
    if (-not $BinDir) {
        Write-Err "ef-api.exe not found. Run: .\scripts\build.ps1 [-Release]"
        exit 1
    }

    Stop-Service-ByName "ef-api" 2>$null

    Write-Info "Starting ef-api (mode=$RunMode)..."
    $env:EF_RUN_MODE = $RunMode
    $logFile = Join-Path $LogDir "ef-api.log"
    $proc = Start-Process -FilePath (Join-Path $BinDir "ef-api.exe") `
        -RedirectStandardOutput $logFile `
        -RedirectStandardError (Join-Path $LogDir "ef-api-err.log") `
        -WindowStyle Hidden -PassThru
    Save-Pid "ef-api" $proc.Id

    Wait-ForPort 6060 "ef-api HTTP"
    Write-Info "Logs: $logFile"
    Write-Info "Swagger UI: http://127.0.0.1:6060/swagger-ui"
    Write-Info "Health: http://127.0.0.1:6060/health"
}

function Deploy-Agent {
    if (-not $BinDir) {
        Write-Err "ef-agent.exe not found. Run: .\scripts\build.ps1 [-Release]"
        exit 1
    }

    Stop-Service-ByName "ef-agent" 2>$null

    $agentId = if ($env:EF__AGENT__AGENT_ID) { $env:EF__AGENT__AGENT_ID } else { $env:COMPUTERNAME.ToLower() }
    $region = if ($env:EF__AGENT__REGION) { $env:EF__AGENT__REGION } else { "local" }
    $controlUrl = if ($env:EF__AGENT__CONTROL_PLANE_URL) { $env:EF__AGENT__CONTROL_PLANE_URL } else { "http://localhost:6060" }
    $tlsSkip = if ($env:EF__AGENT__TLS_SKIP_VERIFY) { $env:EF__AGENT__TLS_SKIP_VERIFY } else { "true" }

    Write-Info "Starting ef-agent (id=$agentId, region=$region)..."
    Write-Info "  -> connecting to: $controlUrl"

    $env:EF_RUN_MODE = $RunMode
    $env:EF__AGENT__AGENT_ID = $agentId
    $env:EF__AGENT__REGION = $region
    $env:EF__AGENT__CONTROL_PLANE_URL = $controlUrl
    $env:EF__AGENT__TLS_SKIP_VERIFY = $tlsSkip

    $logFile = Join-Path $LogDir "ef-agent.log"
    $proc = Start-Process -FilePath (Join-Path $BinDir "ef-agent.exe") `
        -RedirectStandardOutput $logFile `
        -RedirectStandardError (Join-Path $LogDir "ef-agent-err.log") `
        -WindowStyle Hidden -PassThru
    Save-Pid "ef-agent" $proc.Id

    Write-Ok "ef-agent started (PID $($proc.Id))"
    Write-Info "Logs: $logFile"
    Write-Info "Agent will discover local USB devices every 30s"
}

function Deploy-Docker {
    Write-Info "Starting full stack via Docker Compose..."
    docker compose --profile full up -d --build
    Write-Ok "Full Docker Compose stack running"
    Write-Host ""
    Write-Info "  API:     http://localhost:8080"
    Write-Info "  Swagger: http://localhost:8080/swagger-ui"
    Write-Info "  NATS:    http://localhost:8222"
}

function Stop-All {
    Write-Info "Stopping EdgeFabric services..."
    Stop-Service-ByName "ef-agent"
    Stop-Service-ByName "ef-api"

    if (Has "docker") {
        $containers = docker compose ps --quiet 2>$null
        if ($containers) {
            Write-Info "Stopping Docker Compose services..."
            docker compose down
        }
    }
    Write-Ok "All services stopped"
}

function Show-Status {
    Write-Host ""
    Write-Host "EdgeFabric Service Status" -ForegroundColor Cyan
    Write-Host "-------------------------------------------"

    # Native processes
    foreach ($svc in @("ef-api", "ef-agent")) {
        $svcPid = Get-SavedPid $svc
        if ($svcPid) {
            try {
                $proc = Get-Process -Id $svcPid -ErrorAction SilentlyContinue
                if ($proc) {
                    Write-Host "  ${svc}:       " -NoNewline; Write-Host "running (PID $svcPid)" -ForegroundColor Green
                } else {
                    Write-Host "  ${svc}:       " -NoNewline; Write-Host "stopped" -ForegroundColor Red
                }
            } catch {
                Write-Host "  ${svc}:       " -NoNewline; Write-Host "stopped" -ForegroundColor Red
            }
        } else {
            Write-Host "  ${svc}:       " -NoNewline; Write-Host "stopped" -ForegroundColor Red
        }
    }

    Write-Host ""

    # Docker services
    if (Has "docker") {
        foreach ($svc in @("postgres", "redis", "nats", "coturn")) {
            try {
                $state = docker compose ps --format '{{.State}}' $svc 2>$null
                if ($state -eq "running") {
                    Write-Host "  ${svc}:$((' ' * (12 - $svc.Length)))" -NoNewline; Write-Host $state -ForegroundColor Green
                } else {
                    $display = if ($state) { $state } else { "not found" }
                    Write-Host "  ${svc}:$((' ' * (12 - $svc.Length)))" -NoNewline; Write-Host $display -ForegroundColor Red
                }
            } catch {
                Write-Host "  ${svc}:$((' ' * (12 - $svc.Length)))" -NoNewline; Write-Host "not found" -ForegroundColor Red
            }
        }
    }

    Write-Host ""

    # Health check
    try {
        $resp = Invoke-WebRequest -Uri "http://localhost:6060/health" -TimeoutSec 3 -ErrorAction SilentlyContinue
        if ($resp.StatusCode -eq 200) {
            Write-Host "  API health:     " -NoNewline; Write-Host "healthy" -ForegroundColor Green
        }
    } catch {
        try {
            $resp = Invoke-WebRequest -Uri "http://localhost:8080/health" -TimeoutSec 3 -ErrorAction SilentlyContinue
            if ($resp.StatusCode -eq 200) {
                Write-Host "  API health:     " -NoNewline; Write-Host "healthy (Docker)" -ForegroundColor Green
            }
        } catch {
            Write-Host "  API health:     " -NoNewline; Write-Host "unreachable" -ForegroundColor Red
        }
    }

    Write-Host ""
}

function Show-Help {
    Write-Host "EdgeFabric Deploy Script" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Usage: .\scripts\deploy.ps1 <command>"
    Write-Host ""
    Write-Host "Commands:"
    Write-Host "  infra     Start infrastructure (Postgres, Redis, NATS, CoTURN)"
    Write-Host "  api       Start infrastructure + API server (native)"
    Write-Host "  agent     Start the edge agent (native)"
    Write-Host "  all       Start everything (infra + api + agent)"
    Write-Host "  docker    Start full stack via Docker Compose"
    Write-Host "  stop      Stop all EdgeFabric services"
    Write-Host "  status    Show status of all services"
    Write-Host ""
    Write-Host "Environment variables:"
    Write-Host "  EF_RUN_MODE                    development|production (default: development)"
    Write-Host "  EF__AGENT__AGENT_ID            Agent identifier (default: computername)"
    Write-Host "  EF__AGENT__REGION              Agent region (default: local)"
    Write-Host "  EF__AGENT__CONTROL_PLANE_URL   API endpoint (default: http://localhost:6060)"
    Write-Host "  EF__AGENT__TLS_SKIP_VERIFY     Skip TLS cert verify (default: true for dev)"
    Write-Host ""
}

# ── Dispatch ──────────────────────────────────────────────────────
try {
    switch ($Command) {
        "infra"  { Deploy-Infra }
        "api"    { Deploy-Infra; Deploy-Api }
        "agent"  { Deploy-Agent }
        "all"    { Deploy-Infra; Deploy-Api; Deploy-Agent }
        "docker" { Deploy-Docker }
        "stop"   { Stop-All }
        "status" { Show-Status }
        "help"   { Show-Help }
    }
} finally {
    Pop-Location
}
