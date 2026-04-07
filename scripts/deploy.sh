#!/usr/bin/env bash
# EdgeFabric — Deploy / Run Script (macOS & Linux)
#
# Usage:
#   ./scripts/deploy.sh infra          # start Postgres, Redis, NATS, CoTURN
#   ./scripts/deploy.sh api            # start API server (native)
#   ./scripts/deploy.sh agent          # start agent (native)
#   ./scripts/deploy.sh all            # infra + api + agent
#   ./scripts/deploy.sh docker         # full stack via docker compose
#   ./scripts/deploy.sh stop           # stop all services
#   ./scripts/deploy.sh status         # check health of all services
#
# Environment overrides:
#   EF_RUN_MODE=production ./scripts/deploy.sh api
#   EF__AGENT__AGENT_ID=mac-01 ./scripts/deploy.sh agent
#
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

COMMAND="${1:-help}"
RUN_MODE="${EF_RUN_MODE:-development}"
LOG_DIR="$PROJECT_ROOT/logs"
PID_DIR="$PROJECT_ROOT/.pids"

mkdir -p "$LOG_DIR" "$PID_DIR"

# ── Determine binary path ────────────────────────────────────────
if [[ -f "target/release/ef-api" ]]; then
  BIN_DIR="target/release"
elif [[ -f "target/debug/ef-api" ]]; then
  BIN_DIR="target/debug"
else
  BIN_DIR=""
fi

# ── Helper functions ──────────────────────────────────────────────
wait_for_port() {
  local port="$1" name="$2" timeout="${3:-30}"
  info "Waiting for $name on port $port..."
  for i in $(seq 1 "$timeout"); do
    if nc -z localhost "$port" 2>/dev/null || (echo >/dev/tcp/localhost/"$port") 2>/dev/null; then
      ok "$name is ready on port $port"
      return 0
    fi
    sleep 1
  done
  err "$name did not start on port $port within ${timeout}s"
  return 1
}

save_pid() {
  echo "$2" > "$PID_DIR/$1.pid"
}

read_pid() {
  local pidfile="$PID_DIR/$1.pid"
  if [[ -f "$pidfile" ]]; then
    cat "$pidfile"
  fi
}

stop_process() {
  local name="$1"
  local pid
  pid="$(read_pid "$name")"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    info "Stopping $name (PID $pid)..."
    kill "$pid" 2>/dev/null || true
    # Wait for graceful shutdown
    for _ in $(seq 1 10); do
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      sleep 1
    done
    # Force kill if still running
    if kill -0 "$pid" 2>/dev/null; then
      warn "Force killing $name..."
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_DIR/$name.pid"
    ok "$name stopped"
  else
    warn "$name is not running"
    rm -f "$PID_DIR/$name.pid"
  fi
}

# ── Commands ──────────────────────────────────────────────────────

deploy_infra() {
  info "Starting infrastructure services via Docker Compose..."
  if ! command -v docker &>/dev/null; then
    err "Docker not found. Run: ./scripts/install.sh"
    exit 1
  fi
  docker compose up -d postgres redis nats coturn
  echo ""
  wait_for_port 5432 "PostgreSQL"
  wait_for_port 6379 "Redis"
  wait_for_port 4222 "NATS"
  wait_for_port 3478 "CoTURN" 10 || true   # CoTURN uses UDP, TCP check may fail
  echo ""
  ok "Infrastructure services running"
}

deploy_api() {
  if [[ -z "$BIN_DIR" ]]; then
    err "ef-api binary not found. Run: ./scripts/build.sh [--release]"
    exit 1
  fi

  # Stop existing instance if running
  stop_process "ef-api" 2>/dev/null || true

  info "Starting ef-api (mode=$RUN_MODE)..."
  EF_RUN_MODE="$RUN_MODE" \
    nohup "$BIN_DIR/ef-api" \
    > "$LOG_DIR/ef-api.log" 2>&1 &
  save_pid "ef-api" $!

  wait_for_port 6060 "ef-api HTTP"
  info "Logs: $LOG_DIR/ef-api.log"
  info "Swagger UI: http://127.0.0.1:6060/swagger-ui"
  info "Health: http://127.0.0.1:6060/health"
}

deploy_agent() {
  if [[ -z "$BIN_DIR" ]]; then
    err "ef-agent binary not found. Run: ./scripts/build.sh [--release]"
    exit 1
  fi

  # Stop existing instance if running
  stop_process "ef-agent" 2>/dev/null || true

  # Default agent config — can be overridden via env vars
  local agent_id="${EF__AGENT__AGENT_ID:-$(hostname | tr '[:upper:]' '[:lower:]')}"
  local region="${EF__AGENT__REGION:-local}"
  local control_url="${EF__AGENT__CONTROL_PLANE_URL:-http://localhost:6060}"
  local tls_skip="${EF__AGENT__TLS_SKIP_VERIFY:-true}"

  info "Starting ef-agent (id=$agent_id, region=$region)..."
  info "  → connecting to: $control_url"

  EF_RUN_MODE="$RUN_MODE" \
  EF__AGENT__AGENT_ID="$agent_id" \
  EF__AGENT__REGION="$region" \
  EF__AGENT__CONTROL_PLANE_URL="$control_url" \
  EF__AGENT__TLS_SKIP_VERIFY="$tls_skip" \
    nohup "$BIN_DIR/ef-agent" \
    > "$LOG_DIR/ef-agent.log" 2>&1 &
  save_pid "ef-agent" $!

  ok "ef-agent started (PID $!)"
  info "Logs: $LOG_DIR/ef-agent.log"
  info "Agent will discover local USB devices every 30s"
}

deploy_docker() {
  info "Starting full stack via Docker Compose..."
  docker compose --profile full up -d --build
  ok "Full Docker Compose stack running"
  echo ""
  info "  API:     http://localhost:8080"
  info "  Swagger: http://localhost:8080/swagger-ui"
  info "  NATS:    http://localhost:8222"
}

stop_all() {
  info "Stopping EdgeFabric services..."
  stop_process "ef-agent"
  stop_process "ef-api"

  if command -v docker &>/dev/null && docker compose ps --quiet 2>/dev/null | grep -q .; then
    info "Stopping Docker Compose services..."
    docker compose down
  fi
  ok "All services stopped"
}

show_status() {
  echo -e "\n${CYAN}EdgeFabric Service Status${NC}"
  echo "─────────────────────────────────────────────────"

  # Native processes
  for svc in ef-api ef-agent; do
    pid="$(read_pid "$svc")"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      echo -e "  $svc:       ${GREEN}running${NC} (PID $pid)"
    else
      echo -e "  $svc:       ${RED}stopped${NC}"
    fi
  done

  echo ""

  # Infrastructure (Docker)
  if command -v docker &>/dev/null; then
    for svc in postgres redis nats coturn; do
      state="$(docker compose ps --format '{{.State}}' "$svc" 2>/dev/null || echo "not found")"
      if [[ "$state" == "running" ]]; then
        echo -e "  $svc:$(printf '%*s' $((12 - ${#svc})) '') ${GREEN}$state${NC}"
      else
        echo -e "  $svc:$(printf '%*s' $((12 - ${#svc})) '') ${RED}$state${NC}"
      fi
    done
  fi

  echo ""

  # Health checks
  if curl -sf http://localhost:6060/health &>/dev/null; then
    echo -e "  API health:     ${GREEN}healthy${NC} (http://localhost:6060/health)"
  elif curl -sf http://localhost:8080/health &>/dev/null; then
    echo -e "  API health:     ${GREEN}healthy${NC} (http://localhost:8080/health — Docker)"
  else
    echo -e "  API health:     ${RED}unreachable${NC}"
  fi

  echo ""
}

# ── Main dispatch ─────────────────────────────────────────────────
case "$COMMAND" in
  infra)    deploy_infra ;;
  api)      deploy_infra; deploy_api ;;
  agent)    deploy_agent ;;
  all)      deploy_infra; deploy_api; deploy_agent ;;
  docker)   deploy_docker ;;
  stop)     stop_all ;;
  status)   show_status ;;
  help|-h|--help)
    echo "EdgeFabric Deploy Script"
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  infra     Start infrastructure (Postgres, Redis, NATS, CoTURN)"
    echo "  api       Start infrastructure + API server (native)"
    echo "  agent     Start the edge agent (native)"
    echo "  all       Start everything (infra + api + agent)"
    echo "  docker    Start full stack via Docker Compose"
    echo "  stop      Stop all EdgeFabric services"
    echo "  status    Show status of all services"
    echo ""
    echo "Environment:"
    echo "  EF_RUN_MODE                    development|production (default: development)"
    echo "  EF__AGENT__AGENT_ID            Agent identifier (default: hostname)"
    echo "  EF__AGENT__REGION              Agent region (default: local)"
    echo "  EF__AGENT__CONTROL_PLANE_URL   API endpoint (default: http://localhost:6060)"
    echo "  EF__AGENT__TLS_SKIP_VERIFY     Skip TLS cert verify (default: true for dev)"
    echo ""
    ;;
  *) err "Unknown command: $COMMAND (run '$0 help' for usage)"; exit 1 ;;
esac
