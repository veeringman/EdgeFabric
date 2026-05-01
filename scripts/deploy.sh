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

# Load standard ports (single source of truth)
# shellcheck disable=SC1091
source "$SCRIPT_DIR/ports.env"

COMMAND="${1:-help}"
RUN_MODE="${EF_RUN_MODE:-development}"
LOG_DIR="$PROJECT_ROOT/logs"
PID_DIR="$PROJECT_ROOT/.pids"
DB_URL="${DATABASE_URL:-postgres://edgefabric:edgefabric@localhost:5432/edgefabric}"
BUILD_ON_START="${EF_BUILD_ON_START:-true}"
BUILD_PROFILE="${EF_BUILD_PROFILE:-debug}"

mkdir -p "$LOG_DIR" "$PID_DIR"

# ── Helper functions ──────────────────────────────────────────────
resolve_bin_dir() {
  if [[ "$BUILD_PROFILE" == "release" ]]; then
    if [[ -f "target/release/ef-api" || -f "target/release/ef-agent" ]]; then
      echo "target/release"
      return
    fi
  fi

  if [[ -f "target/debug/ef-api" || -f "target/debug/ef-agent" ]]; then
    echo "target/debug"
  elif [[ -f "target/release/ef-api" || -f "target/release/ef-agent" ]]; then
    echo "target/release"
  else
    echo ""
  fi
}

build_service_binary() {
  local pkg="$1"

  if [[ "${BUILD_ON_START,,}" != "true" ]]; then
    info "Skipping build for $pkg (EF_BUILD_ON_START=$BUILD_ON_START)"
    return
  fi

  info "Building $pkg (profile=$BUILD_PROFILE)..."
  if [[ "$BUILD_PROFILE" == "release" ]]; then
    cargo build -p "$pkg" --release
  else
    cargo build -p "$pkg"
  fi
}

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
  wait_for_port "$EF_PORT_POSTGRES" "PostgreSQL"
  wait_for_port "$EF_PORT_REDIS" "Redis"
  wait_for_port "$EF_PORT_NATS" "NATS"
  wait_for_port "$EF_PORT_COTURN" "CoTURN" 10 || true   # CoTURN uses UDP, TCP check may fail
  echo ""
  ok "Infrastructure services running"
}

deploy_api() {
  build_service_binary "ef-api"

  local bin_dir
  bin_dir="$(resolve_bin_dir)"

  if [[ -z "$bin_dir" || ! -x "$bin_dir/ef-api" ]]; then
    err "ef-api binary not found. Run: ./scripts/build.sh [--release]"
    exit 1
  fi

  # Stop existing instance if running
  stop_process "ef-api" 2>/dev/null || true

  info "Starting ef-api (mode=$RUN_MODE)..."
  EF_RUN_MODE="$RUN_MODE" \
    nohup "$bin_dir/ef-api" \
    > "$LOG_DIR/ef-api.log" 2>&1 &
  save_pid "ef-api" $!

  wait_for_port "$EF_PORT_API_HTTP" "ef-api HTTP"
  info "Logs: $LOG_DIR/ef-api.log"
  info "Swagger UI: ${EF_URL_API}/swagger-ui"
  info "Health: ${EF_URL_API}/health"
}

deploy_agent() {
  build_service_binary "ef-agent"

  local bin_dir
  bin_dir="$(resolve_bin_dir)"

  if [[ -z "$bin_dir" || ! -x "$bin_dir/ef-agent" ]]; then
    err "ef-agent binary not found. Run: ./scripts/build.sh [--release]"
    exit 1
  fi

  # Stop existing instance if running
  stop_process "ef-agent" 2>/dev/null || true

  # Default agent config — can be overridden via env vars
  local agent_id="${EF__AGENT__AGENT_ID:-$(hostname | tr '[:upper:]' '[:lower:]')}"
  local region="${EF__AGENT__REGION:-local}"
  local control_url="${EF__AGENT__CONTROL_PLANE_URL:-$EF_URL_API}"
  local tls_skip="${EF__AGENT__TLS_SKIP_VERIFY:-true}"
  info "Starting ef-agent (id=$agent_id, region=$region)..."
  info "  → connecting to: $control_url"

  EF_RUN_MODE="$RUN_MODE" \
  EF__AGENT__AGENT_ID="$agent_id" \
  EF__AGENT__REGION="$region" \
  EF__AGENT__CONTROL_PLANE_URL="$control_url" \
  EF__AGENT__TLS_SKIP_VERIFY="$tls_skip" \
    nohup "$bin_dir/ef-agent" \
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
  stop_process "ef-dashboard"
  stop_process "ef-agent"
  stop_process "ef-api"

  if command -v docker &>/dev/null && docker compose ps --quiet 2>/dev/null | grep -q .; then
    info "Stopping Docker Compose services..."
    docker compose down
  fi
  ok "All services stopped"
}

deploy_dashboard() {
  if ! command -v trunk &>/dev/null; then
    err "trunk not found. Install with: cargo install trunk"
    exit 1
  fi

  # Stop existing instance if running
  stop_process "ef-dashboard" 2>/dev/null || true

  local dash_dir="$PROJECT_ROOT/crates/ef-dashboard"
  local port="${EF_DASHBOARD_PORT:-$EF_PORT_DASHBOARD}"

  info "Starting dashboard (port=$port, proxy via Trunk.toml)..."

  cd "$dash_dir"
  nohup trunk serve \
    --address 0.0.0.0 \
    --port "$port" \
    > "$LOG_DIR/ef-dashboard.log" 2>&1 &
  save_pid "ef-dashboard" $!
  cd "$PROJECT_ROOT"

  wait_for_port "$port" "ef-dashboard"
  info "Dashboard: http://0.0.0.0:$port"
  info "Logs: $LOG_DIR/ef-dashboard.log"
}

show_status() {
  echo -e "\n${CYAN}EdgeFabric Service Status${NC}"
  echo "─────────────────────────────────────────────────"

  # Native processes
  for svc in ef-api ef-agent ef-dashboard; do
    pid="$(read_pid "$svc")"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      echo -e "  $svc:       ${GREEN}running${NC} (PID $pid)"
    else
      echo -e "  $svc:       ${RED}stopped${NC}"
    fi
  done

  echo ""

  # Infrastructure (Docker) — pager disabled
  if command -v docker &>/dev/null; then
    PAGER=cat DOCKER_CLI_HINTS=false docker compose ps --format 'table {{.Service}}\t{{.State}}\t{{.Status}}' 2>/dev/null | cat || true
  fi

  echo ""

  # Health checks
  if curl -sf "${EF_URL_API}/health" &>/dev/null; then
    echo -e "  API health:     ${GREEN}healthy${NC} (${EF_URL_API}/health)"
  else
    echo -e "  API health:     ${RED}unreachable${NC}"
  fi

  echo ""
}

# ── Seed / Unseed demo devices ────────────────────────────────────
seed_devices() {
  info "Inserting demo/seed devices into database..."
  PGPASSWORD="${DB_URL##*:\/\/*:}" PGPASSWORD=edgefabric psql -X -h localhost -U edgefabric -d edgefabric -P pager=off <<'SQL'
INSERT INTO devices (id, name, os, os_version, kind, status, region, serial, capabilities) VALUES
  ('d0000000-0000-0000-0000-000000000001', 'Pixel 8 Pro',       'android', '14',      'real',    'available', 'us-west', 'R5CT900ABCD1',                  ARRAY['camera','nfc','5g']),
  ('d0000000-0000-0000-0000-000000000002', 'Samsung Galaxy S24', 'android', '14',      'real',    'available', 'us-west', 'R3CR900EFGH2',                  ARRAY['camera','nfc','5g']),
  ('d0000000-0000-0000-0000-000000000003', 'Android Emu API 34', 'android', '14',      'virtual', 'available', 'us-west', 'emulator-5554',                 ARRAY['camera']),
  ('d0000000-0000-0000-0000-000000000004', 'iPhone 15 Pro',      'ios',     '17.4',    'real',    'available', 'us-east', '00008030-001234560ABC001E',      ARRAY['camera','face_id','5g']),
  ('d0000000-0000-0000-0000-000000000005', 'iPhone SE (3rd)',     'ios',     '17.4',    'real',    'offline',   'us-east', '00008030-00AABBCC0DEF002E',      ARRAY['camera','touch_id']),
  ('d0000000-0000-0000-0000-000000000006', 'iOS Sim 17',          'ios',     '17.4',   'virtual', 'available', 'us-east', 'AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE', ARRAY['camera']),
  ('d0000000-0000-0000-0000-000000000007', 'RPi 4 Gateway',       'iot',     'bullseye','real',   'available', 'eu-west', '192.168.1.100',                  ARRAY['gpio','bluetooth','wifi']),
  ('d0000000-0000-0000-0000-000000000008', 'ESP32 DevKit',         'iot',     'esp-idf5','real',  'offline',   'eu-west', '192.168.1.101',                  ARRAY['wifi','bluetooth'])
ON CONFLICT (id) DO NOTHING;
SQL
  ok "Seed devices inserted (8 demo devices with d0000000-* IDs)"
}

unseed_devices() {
  info "Removing demo/seed devices from database..."
  PGPASSWORD=edgefabric psql -X -h localhost -U edgefabric -d edgefabric -P pager=off -c \
    "DELETE FROM devices WHERE id::text LIKE 'd0000000-0000-0000-0000-00000000000%';"
  ok "Seed devices removed"
}

# ── Logs ──────────────────────────────────────────────────────────
follow_logs() {
  local svc="${1:-all}"
  case "$svc" in
    api|ef-api)         tail -F "$LOG_DIR/ef-api.log" ;;
    agent|ef-agent)     tail -F "$LOG_DIR/ef-agent.log" ;;
    dash|ef-dashboard)  tail -F "$LOG_DIR/ef-dashboard.log" ;;
    all|"")             tail -F "$LOG_DIR"/ef-*.log ;;
    infra|docker)       PAGER=cat docker compose logs -f --tail=100 ;;
    *) err "Unknown service for logs: $svc (api|agent|dashboard|infra|all)"; exit 1 ;;
  esac
}

# ── Restart ───────────────────────────────────────────────────────
restart_service() {
  local svc="${1:-all}"
  case "$svc" in
    api|ef-api)         stop_process ef-api;       deploy_api ;;
    agent|ef-agent)     stop_process ef-agent;     deploy_agent ;;
    dash|dashboard|ef-dashboard)  stop_process ef-dashboard; deploy_dashboard ;;
    infra)              docker compose restart postgres redis nats coturn ;;
    all|"")
      stop_process ef-dashboard; stop_process ef-agent; stop_process ef-api
      deploy_infra; deploy_api; deploy_agent; deploy_dashboard
      ;;
    *) err "Unknown service for restart: $svc"; exit 1 ;;
  esac
}

# ── VeerOS (folded VMs) management ────────────────────────────────
veeros_bin() {
  local root="${EF__VEEROS__ROOT_DIR:-/home/vijay/rnd/VeerOS}"
  if   [[ -x "$root/target/release/fold" ]]; then echo "$root/target/release/fold"
  elif [[ -x "$root/target/debug/fold"   ]]; then echo "$root/target/debug/fold"
  else err "fold binary not found under $root/target/{release,debug}"; return 1
  fi
}

veeros_cmd() {
  local sub="${1:-list}"; shift || true
  local bin; bin="$(veeros_bin)" || exit 1
  case "$sub" in
    net)
      "$SCRIPT_DIR/veeros-net.sh" "${1:-status}"
      ;;
    list)       PAGER=cat "$bin" list ;;
    logs)       PAGER=cat "$bin" logs "$@" ;;
    stop)       "$bin" stop "$@" ;;
    rm)         "$bin" rm "$@" ;;
    stop-all)
      info "Stopping all running fold VMs..."
      "$bin" list 2>/dev/null | awk 'NR>1 && $0 ~ /running/ {print $1}' | while read -r vm; do
        [[ -n "$vm" ]] && { info "  stop $vm"; "$bin" stop "$vm" || true; }
      done
      ok "All running fold VMs stopped"
      ;;
    clean)
      info "Removing stopped fold VMs and stale manifests..."
      "$bin" list 2>/dev/null | awk 'NR>1 && $0 ~ /stopped/ {print $1}' | while read -r vm; do
        [[ -n "$vm" ]] && { info "  rm $vm"; "$bin" rm "$vm" || true; rm -f "/tmp/edgefabric-${vm}.toml" "/tmp/edgefabric-${vm}.sensor"; }
      done
      ok "Stopped fold VMs removed"
      ;;
    *) err "Unknown veeros subcommand: $sub (net|list|logs|stop|stop-all|rm|clean)"; exit 1 ;;
  esac
}

# ── Database reset (dev only) ─────────────────────────────────────
reset_db() {
  warn "This will DROP and recreate the edgefabric database. Dev only."
  read -r -p "Type 'yes' to confirm: " answer
  [[ "$answer" == "yes" ]] || { info "Aborted."; return 0; }
  stop_process ef-api   2>/dev/null || true
  stop_process ef-agent 2>/dev/null || true
  PGPASSWORD=edgefabric psql -h localhost -U edgefabric -d postgres -c "DROP DATABASE IF EXISTS edgefabric;"
  PGPASSWORD=edgefabric psql -h localhost -U edgefabric -d postgres -c "CREATE DATABASE edgefabric OWNER edgefabric;"
  "$SCRIPT_DIR/migrate.sh"
  ok "Database reset complete"
}

# ── Main dispatch ─────────────────────────────────────────────────
case "$COMMAND" in
  infra)      deploy_infra ;;
  api)        deploy_infra; deploy_api ;;
  agent)      deploy_agent ;;
  dashboard)  deploy_dashboard ;;
  all)        deploy_infra; deploy_api; deploy_agent; deploy_dashboard ;;
  docker)     deploy_docker ;;
  seed)       seed_devices ;;
  unseed)     unseed_devices ;;
  stop)       stop_all ;;
  restart)    shift || true; restart_service "${1:-all}" ;;
  logs)       shift || true; follow_logs "${1:-all}" ;;
  veeros)     shift || true; veeros_cmd "$@" ;;
  reset-db)   reset_db ;;
  status)     show_status ;;
  ports)      cat "$SCRIPT_DIR/ports.env" ;;
  help|-h|--help)
    echo "EdgeFabric Deploy Script"
    echo ""
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Commands:"
    echo "  infra              Start infrastructure (Postgres, Redis, NATS, CoTURN)"
    echo "  api                Start infrastructure + API server (native)"
    echo "  agent              Start the edge agent (native)"
    echo "  dashboard          Start the dashboard (Trunk dev server)"
    echo "  all                Start everything (infra + api + agent + dashboard)"
    echo "  docker             Start full stack via Docker Compose"
    echo "  restart <svc>      Restart api|agent|dashboard|infra|all"
    echo "  logs <svc>         Tail logs for api|agent|dashboard|infra|all"
    echo "  stop               Stop all EdgeFabric services"
    echo "  status             Show status of all services"
    echo "  ports              Print standard port map"
    echo "  seed|unseed        Insert/remove demo devices"
    echo "  reset-db           Drop+recreate database and re-run migrations (dev only)"
    echo "  veeros <sub>       Manage fold VMs: net|list|logs|stop|stop-all|rm|clean"
    echo ""
    echo "Environment:"
    echo "  EF_RUN_MODE                    development|production (default: development)"
    echo "  EF__AGENT__AGENT_ID            Agent identifier (default: hostname)"
    echo "  EF__AGENT__REGION              Agent region (default: local)"
    echo "  EF__AGENT__CONTROL_PLANE_URL   API endpoint (default: \$EF_URL_API)"
    echo "  EF__AGENT__TLS_SKIP_VERIFY     Skip TLS cert verify (default: true for dev)"
    echo "  EF_DASHBOARD_PORT              Dashboard port (default: \$EF_PORT_DASHBOARD)"
    echo "  EF_DASHBOARD_API_PROXY         API proxy target (default: \$EF_URL_API)"
    echo ""
    echo "Standard ports are defined in scripts/ports.env (run '$0 ports' to view)."
    echo ""
    ;;
  *) err "Unknown command: $COMMAND (run '$0 help' for usage)"; exit 1 ;;
esac
