#!/usr/bin/env bash
# EdgeFabric — Deploy / Run Script (macOS & Linux)
#
# Usage:
#   ./scripts/deploy.sh infra          # start Postgres, Redis, NATS, CoTURN
#   ./scripts/deploy.sh infra-local    # start infra from local host services/binaries
#   ./scripts/deploy.sh infra-stop     # stop infra services only
#   ./scripts/deploy.sh api            # start API server (native)
#   ./scripts/deploy.sh agent          # start agent (native)
#   ./scripts/deploy.sh all            # infra + api + agent
#   ./scripts/deploy.sh all-local      # local infra + api + agent + dashboard
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
OS_NAME="$(uname -s)"
IS_MACOS=false
if [[ "$OS_NAME" == "Darwin" ]]; then
  IS_MACOS=true
fi

if [[ "$IS_MACOS" == "true" ]]; then
  if [[ -n "${EF_INFRA_MODE:-}" && "${EF_INFRA_MODE}" != "local" ]]; then
    warn "macOS always uses local infrastructure; ignoring EF_INFRA_MODE=${EF_INFRA_MODE}"
  fi
  INFRA_MODE="local"
else
  INFRA_MODE="${EF_INFRA_MODE:-docker}"
fi
BIN_PROFILE="${EF_BIN_PROFILE:-auto}"
LOG_DIR="$PROJECT_ROOT/logs"
PID_DIR="$PROJECT_ROOT/.pids"
DB_URL="${DATABASE_URL:-postgres://edgefabric:edgefabric@localhost:5432/edgefabric}"
LOCAL_INFRA_BREW_FILE="$PID_DIR/local-infra.brew"
BUILD_ON_START="${EF_BUILD_ON_START:-true}"
BUILD_PROFILE="${EF_BUILD_PROFILE:-debug}"

mkdir -p "$LOG_DIR" "$PID_DIR"

run_compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
    return
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    docker-compose "$@"
    return
  fi

  err "Docker Compose is not available. Install Docker Compose v2 plugin or docker-compose binary."
  return 127
}

# ── Determine binary path ────────────────────────────────────────
resolve_bin_dir() {
  case "$BIN_PROFILE" in
    debug)
      [[ -f "target/debug/ef-api" ]] && { echo "target/debug"; return 0; }
      ;;
    release)
      [[ -f "target/release/ef-api" ]] && { echo "target/release"; return 0; }
      ;;
    auto)
      # In development mode prefer debug to avoid stale release binaries
      # compiled against older embedded SQLx migrations.
      if [[ "$RUN_MODE" == "development" ]]; then
        [[ -f "target/debug/ef-api" ]] && { echo "target/debug"; return 0; }
        [[ -f "target/release/ef-api" ]] && { echo "target/release"; return 0; }
      else
        [[ -f "target/release/ef-api" ]] && { echo "target/release"; return 0; }
        [[ -f "target/debug/ef-api" ]] && { echo "target/debug"; return 0; }
      fi
      ;;
    *)
      err "Invalid EF_BIN_PROFILE='$BIN_PROFILE' (use auto|debug|release)"
      exit 1
      ;;
  esac

  echo ""
}

BIN_DIR="$(resolve_bin_dir)"

# ── Helper functions ──────────────────────────────────────────────
build_service_binary() {
  local pkg="$1"
  local build_on_start_lc
  build_on_start_lc="$(printf '%s' "$BUILD_ON_START" | tr '[:upper:]' '[:lower:]')"

  if [[ "$build_on_start_lc" != "true" ]]; then
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

port_open() {
  local port="$1"
  nc -z localhost "$port" 2>/dev/null || (echo >/dev/tcp/localhost/"$port") 2>/dev/null
}

mark_brew_service_started() {
  local svc="$1"
  touch "$LOCAL_INFRA_BREW_FILE"
  if ! grep -qx "$svc" "$LOCAL_INFRA_BREW_FILE" 2>/dev/null; then
    echo "$svc" >> "$LOCAL_INFRA_BREW_FILE"
  fi
}

start_brew_service_if_possible() {
  local svc="$1"
  if ! command -v brew &>/dev/null; then
    return 1
  fi

  if brew services start "$svc" >/dev/null 2>&1; then
    info "Started Homebrew service: $svc"
    mark_brew_service_started "$svc"
    return 0
  fi

  return 1
}

deploy_infra_local() {
  info "Starting infrastructure services from local host tools (no Docker)..."

  # PostgreSQL
  if port_open 5432; then
    ok "PostgreSQL already running on 5432"
  else
    local pg_started=false
    for svc in postgresql@17 postgresql@16 postgresql@15 postgresql@14 postgresql; do
      if start_brew_service_if_possible "$svc"; then
        pg_started=true
        break
      fi
    done

    if [[ "$pg_started" != true ]]; then
      err "PostgreSQL is not running and could not be started locally."
      err "Install/start it (example): brew install postgresql@17 && brew services start postgresql@17"
      exit 1
    fi

    wait_for_port 5432 "PostgreSQL"
  fi

  # Redis
  if port_open 6379; then
    ok "Redis already running on 6379"
  else
    if ! start_brew_service_if_possible "redis"; then
      err "Redis is not running and could not be started locally."
      err "Install/start it (example): brew install redis && brew services start redis"
      exit 1
    fi
    wait_for_port 6379 "Redis"
  fi

  # NATS
  if port_open 4222; then
    ok "NATS already running on 4222"
  else
    if start_brew_service_if_possible "nats-server"; then
      wait_for_port 4222 "NATS"
    elif command -v nats-server &>/dev/null; then
      info "Starting nats-server directly (JetStream + monitoring)..."
      stop_process "ef-nats" 2>/dev/null || true
      nohup nats-server --jetstream --http_port 8222 > "$LOG_DIR/nats-server.log" 2>&1 &
      save_pid "ef-nats" $!
      wait_for_port 4222 "NATS"
      wait_for_port 8222 "NATS monitor" 10 || true
      info "Logs: $LOG_DIR/nats-server.log"
    else
      err "NATS is not running and could not be started locally."
      err "Install/start it (example): brew install nats-server && brew services start nats-server"
      exit 1
    fi
  fi

  # CoTURN (optional for some streaming paths)
  if port_open 3478; then
    ok "CoTURN already listening on 3478"
  else
    if start_brew_service_if_possible "coturn"; then
      wait_for_port 3478 "CoTURN" 10 || true
    elif command -v turnserver &>/dev/null; then
      info "Starting turnserver directly..."
      stop_process "ef-coturn" 2>/dev/null || true
      nohup turnserver \
        --log-file=stdout \
        --min-port=49152 \
        --max-port=49200 \
        --realm=edgefabric \
        --user=edgefabric:edgefabric \
        --lt-cred-mech \
        > "$LOG_DIR/coturn.log" 2>&1 &
      save_pid "ef-coturn" $!
      wait_for_port 3478 "CoTURN" 10 || true
      info "Logs: $LOG_DIR/coturn.log"
    else
      warn "CoTURN not installed. Continuing without local TURN server."
      warn "Install if needed: brew install coturn"
    fi
  fi

  ok "Local infrastructure is ready"
}

deploy_infra_auto() {
  case "$INFRA_MODE" in
    local)
      deploy_infra_local
      ;;
    docker|"")
      deploy_infra
      ;;
    *)
      err "Invalid EF_INFRA_MODE='$INFRA_MODE' (use 'docker' or 'local')"
      exit 1
      ;;
  esac
}

# ── Commands ──────────────────────────────────────────────────────

deploy_infra() {
  if [[ "$IS_MACOS" == "true" ]]; then
    warn "macOS is configured for local infrastructure; running infra-local instead of Docker Compose"
    deploy_infra_local
    return
  fi

  info "Starting infrastructure services via Docker Compose..."
  if ! command -v docker &>/dev/null; then
    err "Docker not found. Run: ./scripts/install.sh"
    exit 1
  fi
  run_compose up -d postgres redis nats coturn
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
  local veeros_root="${EF__VEEROS__ROOT_DIR:-}"
  local veeros_vm_script="${EF__VEEROS__VM_SCRIPT_PATH:-}"
  local veeros_connect_path="${EF__VEEROS__VEER_CONNECT_PATH:-}"
  local veeros_target="${EF__VEEROS__DEFAULT_TARGET:-}"

  # macOS convenience: auto-detect a common local VeerOS checkout so agent
  # VeerOS endpoints work without requiring Linux-style /home/... paths.
  if [[ "$(uname -s)" == "Darwin" ]] && [[ -z "$veeros_root" ]]; then
    local mac_veeros_root="$HOME/VeerOS"
    local mac_vm_script="$mac_veeros_root/scripts/veeros-vm"
    local mac_connect_path="$mac_veeros_root/target/debug/veer-connect"
    if [[ -d "$mac_veeros_root" ]] && [[ -x "$mac_vm_script" ]]; then
      veeros_root="$mac_veeros_root"
      veeros_vm_script="$mac_vm_script"
      if [[ -x "$mac_connect_path" ]]; then
        veeros_connect_path="$mac_connect_path"
      fi
      if [[ -z "$veeros_target" ]]; then
        veeros_target="esp32c6"
      fi
      info "Auto-detected VeerOS at $veeros_root"
    fi
  fi

  info "Starting ef-agent (id=$agent_id, region=$region)..."
  info "  → connecting to: $control_url"
  if [[ -n "$veeros_vm_script" ]]; then
    info "  → VeerOS script: $veeros_vm_script"
  fi

  if [[ -n "$veeros_root" ]]; then
    export EF__VEEROS__ROOT_DIR="$veeros_root"
  fi
  if [[ -n "$veeros_vm_script" ]]; then
    export EF__VEEROS__VM_SCRIPT_PATH="$veeros_vm_script"
  fi
  if [[ -n "$veeros_connect_path" ]]; then
    export EF__VEEROS__VEER_CONNECT_PATH="$veeros_connect_path"
  fi
  if [[ -n "$veeros_target" ]]; then
    export EF__VEEROS__DEFAULT_TARGET="$veeros_target"
  fi

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
  if [[ "$IS_MACOS" == "true" ]]; then
    warn "macOS is configured for local runtime; running all-local instead of Docker Compose"
    INFRA_MODE="local"
    deploy_infra_auto
    deploy_api
    deploy_agent
    deploy_dashboard
    return
  fi

  info "Starting full stack via Docker Compose..."
  run_compose --profile full up -d --build
  ok "Full Docker Compose stack running"
  echo ""
  info "  API:     http://localhost:8080"
  info "  Swagger: http://localhost:8080/swagger-ui"
  info "  NATS:    http://localhost:8222"
}

stop_all() {
  info "Stopping EdgeFabric services..."
  stop_infra
  stop_process "ef-dashboard"
  stop_process "ef-agent"
  stop_process "ef-api"
  ok "All services stopped"
}

stop_infra() {
  info "Stopping infrastructure services..."
  stop_process "ef-coturn" 2>/dev/null || true
  stop_process "ef-nats" 2>/dev/null || true

  if command -v docker &>/dev/null && run_compose ps --quiet 2>/dev/null | grep -q .; then
    info "Stopping Docker Compose infra services..."
    run_compose stop postgres redis nats coturn >/dev/null 2>&1 || true
  fi

  if command -v brew &>/dev/null && [[ -f "$LOCAL_INFRA_BREW_FILE" ]]; then
    info "Stopping Homebrew infra services started by this script..."
    while IFS= read -r svc; do
      [[ -n "$svc" ]] || continue
      brew services stop "$svc" >/dev/null 2>&1 || true
    done < "$LOCAL_INFRA_BREW_FILE"
    rm -f "$LOCAL_INFRA_BREW_FILE"
  fi

  ok "Infrastructure services stopped"
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
    PAGER=cat DOCKER_CLI_HINTS=false run_compose ps --format 'table {{.Service}}\t{{.State}}\t{{.Status}}' 2>/dev/null | cat || true
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
    infra|docker)       PAGER=cat run_compose logs -f --tail=100 ;;
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
    infra)              run_compose restart postgres redis nats coturn ;;
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
  infra-local) deploy_infra_local ;;
  infra-stop) stop_infra ;;
  api)        deploy_infra_auto; deploy_api ;;
  api-local)  INFRA_MODE="local"; deploy_infra_auto; deploy_api ;;
  agent)      deploy_agent ;;
  dashboard)  deploy_dashboard ;;
  all)        deploy_infra_auto; deploy_api; deploy_agent; deploy_dashboard ;;
  all-local)  INFRA_MODE="local"; deploy_infra_auto; deploy_api; deploy_agent; deploy_dashboard ;;
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
    echo "  infra-local        Start infrastructure using local host services/binaries"
    echo "  infra-stop         Stop infrastructure only (Docker/local)"
    echo "  api                Start infrastructure + API server (native)"
    echo "  api-local          Start local infrastructure + API server (native)"
    echo "  agent              Start the edge agent (native)"
    echo "  dashboard          Start the dashboard (Trunk dev server)"
    echo "  all                Start everything (infra + api + agent + dashboard)"
    echo "  all-local          Start everything with local infra (no Docker)"
    echo "  docker             Start full stack via Docker Compose (macOS: runs all-local)"
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
    echo "  EF_INFRA_MODE                  docker|local (Linux default: docker; macOS: always local)"
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
