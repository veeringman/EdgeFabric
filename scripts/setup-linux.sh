#!/usr/bin/env bash
# EdgeFabric -- Linux Control Plane & Dashboard Setup
#
# Deploys the complete EdgeFabric stack on Ubuntu/Debian:
#   - Infrastructure (Postgres, Redis, NATS, CoTURN)
#   - Database migrations
#   - API server (control plane) on 0.0.0.0:6060
#   - Edge agent (local device discovery)
#   - Dashboard (WASM via trunk)
#
# Usage:
#   ./scripts/setup-linux.sh                 # install + build + deploy all
#   ./scripts/setup-linux.sh --agent-only    # agent node only (API runs elsewhere)
#   ./scripts/setup-linux.sh --step install  # just install prerequisites
#   ./scripts/setup-linux.sh --step build    # just build
#   ./scripts/setup-linux.sh --step deploy   # just deploy (assumes built)
#   ./scripts/setup-linux.sh --step status   # show service status
#   ./scripts/setup-linux.sh --step stop     # stop all services
#   ./scripts/setup-linux.sh --release       # build in release mode
#
# Environment overrides:
#   EF__AGENT__CONTROL_PLANE_URL=https://remote:6060 ./scripts/setup-linux.sh --agent-only
#   EF__AGENT__AGENT_ID=linux-rack-01 ./scripts/setup-linux.sh
#   EF__DATABASE__URL=postgres://user:pass@host/db ./scripts/setup-linux.sh
#
set -euo pipefail

# ── Colours ───────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'
info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()      { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()     { echo -e "${RED}[ERROR]${NC} $*" >&2; }
section() { echo -e "\n${BOLD}${CYAN}── $* ──${NC}\n"; }

# ── Parse arguments ───────────────────────────────────────────────
AGENT_ONLY=false
RELEASE=false
STEP="all"
for arg in "$@"; do
  case "$arg" in
    --agent-only)   AGENT_ONLY=true ;;
    --release|-r)   RELEASE=true ;;
    --step)         : ;; # value captured below
    install|build|deploy|status|stop|check)
      STEP="$arg" ;;
    -h|--help)
      sed -n '2,25p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) err "Unknown flag: $arg"; exit 1 ;;
  esac
done

# ── Verify Linux ──────────────────────────────────────────────────
if [[ "$(uname)" != "Linux" ]]; then
  err "This script is for Linux. Use ./scripts/setup-mac.sh on macOS."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

ARCH="$(uname -m)"
DISTRO="$(lsb_release -is 2>/dev/null || echo "Linux")"
DISTRO_VER="$(lsb_release -rs 2>/dev/null || echo "?")"
RAM_GB=$(( $(grep MemTotal /proc/meminfo | awk '{print $2}') / 1048576 ))
LAN_IP="$(hostname -I 2>/dev/null | awk '{print $1}' || echo "127.0.0.1")"

LOG_DIR="$PROJECT_ROOT/logs"
PID_DIR="$PROJECT_ROOT/.pids"
mkdir -p "$LOG_DIR" "$PID_DIR"

DB_URL="${EF__DATABASE__URL:-postgres://edgefabric:edgefabric@localhost:5432/edgefabric}"

echo -e "${BOLD}${CYAN}"
echo "  ╔══════════════════════════════════════════════════╗"
echo "  ║   EdgeFabric -- Linux Control Plane Setup        ║"
echo "  ╚══════════════════════════════════════════════════╝"
echo -e "${NC}"
info "$DISTRO $DISTRO_VER ($ARCH), ${RAM_GB} GB RAM, LAN: $LAN_IP"
info "Project root: $PROJECT_ROOT"
echo ""

# ── Helpers ───────────────────────────────────────────────────────
has() { command -v "$1" &>/dev/null; }

pkg_install() {
  if has apt-get; then
    sudo apt-get update -qq && sudo apt-get install -y "$@"
  elif has dnf; then
    sudo dnf install -y "$@"
  elif has pacman; then
    sudo pacman -S --noconfirm "$@"
  else
    err "No supported package manager (apt/dnf/pacman)"
    exit 1
  fi
}

wait_for_port() {
  local port="$1" name="$2" timeout="${3:-30}"
  info "Waiting for $name on port $port..."
  for _ in $(seq 1 "$timeout"); do
    if nc -z localhost "$port" 2>/dev/null || (echo >/dev/tcp/localhost/"$port") 2>/dev/null; then
      ok "$name is ready on port $port"
      return 0
    fi
    sleep 1
  done
  err "$name did not start on port $port within ${timeout}s"
  return 1
}

stop_service() {
  local name="$1"
  local pidfile="$PID_DIR/$name.pid"
  if [[ -f "$pidfile" ]]; then
    local pid
    pid="$(cat "$pidfile")"
    if kill -0 "$pid" 2>/dev/null; then
      info "Stopping $name (PID $pid)..."
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 10); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 1
      done
      if kill -0 "$pid" 2>/dev/null; then
        warn "Force killing $name..."
        kill -9 "$pid" 2>/dev/null || true
      fi
      ok "$name stopped"
    else
      warn "$name is not running"
    fi
    rm -f "$pidfile"
  else
    warn "$name is not running"
  fi
}

# ══════════════════════════════════════════════════════════════════
# STEP 1: INSTALL PREREQUISITES
# ══════════════════════════════════════════════════════════════════
do_install() {
  section "1/4  Installing prerequisites"

  # ── Core build tools ────────────────────────────────────────────
  info "Checking core build tools..."
  pkg_install build-essential pkg-config curl git libssl-dev
  ok "Core build tools ready"

  # ── Rust ────────────────────────────────────────────────────────
  if ! has rustup; then
    info "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
  else
    RUST_VER="$(rustc --version | awk '{print $2}')"
    RUST_MINOR="$(echo "$RUST_VER" | cut -d. -f2)"
    if [[ "$RUST_MINOR" -lt 85 ]]; then
      info "Rust $RUST_VER is too old (need >= 1.85). Updating..."
      rustup update stable
    fi
  fi
  ok "Rust $(rustc --version | awk '{print $2}')"

  # ── Protobuf compiler ──────────────────────────────────────────
  if ! has protoc; then
    pkg_install protobuf-compiler
  fi
  ok "protoc $(protoc --version | awk '{print $2}')"

  # ── Docker ──────────────────────────────────────────────────────
  if [[ "$AGENT_ONLY" == false ]]; then
    if ! has docker; then
      info "Installing Docker..."
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      warn "Log out and back in for Docker group, or run: newgrp docker"
    fi
    ok "Docker $(docker --version | awk '{print $3}' | tr -d ',')"

    # Docker Compose (v2 plugin)
    if ! docker compose version &>/dev/null; then
      info "Installing Docker Compose plugin..."
      sudo apt-get install -y docker-compose-plugin 2>/dev/null || true
    fi
    ok "Docker Compose $(docker compose version --short 2>/dev/null || echo 'available')"
  fi

  # ── PostgreSQL (local, skip Docker dep for Postgres) ────────────
  if [[ "$AGENT_ONLY" == false ]]; then
    if ! has psql; then
      info "Installing PostgreSQL client..."
      pkg_install postgresql-client
    fi
    # Check if Postgres service is running
    if ss -tlnp 2>/dev/null | grep -q ':5432'; then
      ok "PostgreSQL already running on port 5432"
    else
      info "No local PostgreSQL detected -- Docker Compose will provide it"
    fi

    # sqlx-cli for migrations
    if ! has sqlx; then
      info "Installing sqlx-cli..."
      cargo install sqlx-cli --no-default-features --features postgres
    fi
    ok "sqlx-cli available"
  fi

  # ── Android (adb) ──────────────────────────────────────────────
  if ! has adb; then
    info "Installing Android platform tools..."
    pkg_install android-tools-adb 2>/dev/null || pkg_install android-tools 2>/dev/null || warn "adb not available in repos"
  fi
  if has adb; then
    ok "adb $(adb version 2>/dev/null | head -1 | awk '{print $NF}' || echo 'installed')"
  fi

  # ── mDNS / Avahi (IoT discovery) ───────────────────────────────
  if ! has avahi-browse; then
    info "Installing avahi-utils..."
    pkg_install avahi-utils 2>/dev/null || true
  fi
  if has avahi-browse; then
    ok "avahi-browse available (IoT mDNS discovery)"
  fi

  echo ""
  ok "All prerequisites installed!"
}

# ══════════════════════════════════════════════════════════════════
# STEP 2: BUILD
# ══════════════════════════════════════════════════════════════════
do_build() {
  section "2/4  Building EdgeFabric"

  # Source cargo env in case install step just added it
  [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"

  # Limit parallel jobs on low-RAM machines
  if [[ $RAM_GB -le 8 ]]; then
    export CARGO_BUILD_JOBS=2
    info "Limiting cargo jobs to 2 (${RAM_GB} GB RAM detected)"
  fi

  BUILD_FLAGS=()
  if [[ "$RELEASE" == true ]]; then
    BUILD_FLAGS+=(--release)
    BIN_DIR="target/release"
    info "Building in RELEASE mode"
  else
    BIN_DIR="target/debug"
    info "Building in DEBUG mode"
  fi

  if [[ "$AGENT_ONLY" == true ]]; then
    info "Building ef-agent only..."
    cargo build -p ef-agent "${BUILD_FLAGS[@]}"
    ok "ef-agent built -> $BIN_DIR/ef-agent"
  else
    info "Building all workspace crates..."
    cargo build "${BUILD_FLAGS[@]}"
    ok "All crates built -> $BIN_DIR/"
  fi
}

# ══════════════════════════════════════════════════════════════════
# STEP 3: DEPLOY
# ══════════════════════════════════════════════════════════════════
do_deploy() {
  section "3/4  Deploying EdgeFabric"

  # Determine binary directory
  if [[ "$RELEASE" == true ]]; then
    BIN_DIR="target/release"
  else
    BIN_DIR="target/debug"
  fi

  if [[ "$AGENT_ONLY" == true ]]; then
    deploy_agent
    return
  fi

  deploy_infra
  deploy_migrations
  deploy_api
  deploy_agent
}

deploy_infra() {
  info "Starting infrastructure services..."

  # Check if local Postgres is already running
  local pg_local=false
  if ss -tlnp 2>/dev/null | grep -q ':5432'; then
    ok "PostgreSQL already running locally on port 5432"
    pg_local=true
  fi

  # Start Docker services (skip Postgres if local is running)
  if [[ "$pg_local" == true ]]; then
    docker compose up -d redis nats coturn 2>&1
  else
    docker compose up -d postgres redis nats coturn 2>&1
    wait_for_port 5432 "PostgreSQL"
  fi

  wait_for_port 6379 "Redis"
  wait_for_port 4222 "NATS"
  ok "Infrastructure services running"

  # Ensure Postgres user and database exist
  if [[ "$pg_local" == true ]]; then
    info "Ensuring edgefabric database exists..."
    sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='edgefabric'" 2>/dev/null | grep -q 1 || \
      sudo -u postgres psql -c "CREATE USER edgefabric WITH PASSWORD 'edgefabric' CREATEDB;" 2>/dev/null || true
    sudo -u postgres psql -lqt 2>/dev/null | grep -qw edgefabric || \
      sudo -u postgres psql -c "CREATE DATABASE edgefabric OWNER edgefabric;" 2>/dev/null || true
    ok "Database ready"
  fi
}

deploy_migrations() {
  info "Running database migrations..."
  DATABASE_URL="$DB_URL" sqlx migrate run --source "$PROJECT_ROOT/migrations" --database-url "$DB_URL"
  ok "Migrations complete"
}

deploy_api() {
  # Stop existing API if running
  stop_service "ef-api" 2>/dev/null || true

  info "Starting ef-api (control plane) on 0.0.0.0:6060..."

  EF_RUN_MODE="${EF_RUN_MODE:-development}" \
  EF__SERVER__HOST="0.0.0.0" \
  EF__DATABASE__URL="$DB_URL" \
    nohup "$BIN_DIR/ef-api" > "$LOG_DIR/ef-api.log" 2>&1 &
  local api_pid=$!
  echo "$api_pid" > "$PID_DIR/ef-api.pid"

  wait_for_port 6060 "ef-api HTTP"

  ok "ef-api running (PID $api_pid)"
  info "  Health:     http://$LAN_IP:6060/health"
  info "  Swagger UI: http://$LAN_IP:6060/swagger-ui"
  info "  Logs:       $LOG_DIR/ef-api.log"
}

deploy_agent() {
  # Stop existing agent if running
  stop_service "ef-agent" 2>/dev/null || true

  local agent_id="${EF__AGENT__AGENT_ID:-$(hostname | tr '[:upper:]' '[:lower:]')}"
  local region="${EF__AGENT__REGION:-local}"
  local control_url="${EF__AGENT__CONTROL_PLANE_URL:-http://localhost:6060}"
  local tls_skip="${EF__AGENT__TLS_SKIP_VERIFY:-true}"

  info "Starting ef-agent (id=$agent_id, region=$region)..."
  info "  -> control plane: $control_url"

  EF_RUN_MODE="${EF_RUN_MODE:-development}" \
  EF__AGENT__AGENT_ID="$agent_id" \
  EF__AGENT__REGION="$region" \
  EF__AGENT__CONTROL_PLANE_URL="$control_url" \
  EF__AGENT__TLS_SKIP_VERIFY="$tls_skip" \
    nohup "$BIN_DIR/ef-agent" > "$LOG_DIR/ef-agent.log" 2>&1 &
  local agent_pid=$!
  echo "$agent_pid" > "$PID_DIR/ef-agent.pid"

  sleep 2
  if kill -0 "$agent_pid" 2>/dev/null; then
    ok "ef-agent running (PID $agent_pid)"
  else
    err "ef-agent failed to start. Check: $LOG_DIR/ef-agent.log"
    return 1
  fi
  info "  Logs: $LOG_DIR/ef-agent.log"
}

# ══════════════════════════════════════════════════════════════════
# STATUS / STOP / CHECK
# ══════════════════════════════════════════════════════════════════
do_status() {
  section "Service Status"

  echo -e "${BOLD}Native Processes:${NC}"
  for svc in ef-api ef-agent; do
    local pidfile="$PID_DIR/$svc.pid"
    if [[ -f "$pidfile" ]]; then
      local pid
      pid="$(cat "$pidfile")"
      if kill -0 "$pid" 2>/dev/null; then
        echo -e "  $svc:       ${GREEN}running${NC} (PID $pid)"
      else
        echo -e "  $svc:       ${RED}stopped${NC} (stale PID)"
      fi
    else
      echo -e "  $svc:       ${RED}stopped${NC}"
    fi
  done
  echo ""

  echo -e "${BOLD}Docker Services:${NC}"
  for svc in postgres redis nats coturn; do
    local state
    state="$(docker compose ps --format '{{.State}}' "$svc" 2>/dev/null || echo "not found")"
    if [[ "$state" == "running" ]]; then
      echo -e "  $svc:$(printf '%*s' $((12 - ${#svc})) '')${GREEN}$state${NC}"
    else
      echo -e "  $svc:$(printf '%*s' $((12 - ${#svc})) '')${RED}$state${NC}"
    fi
  done

  # Check for local Postgres (non-Docker)
  if ss -tlnp 2>/dev/null | grep -q ':5432'; then
    local pg_docker
    pg_docker="$(docker compose ps --format '{{.State}}' postgres 2>/dev/null || echo "")"
    if [[ "$pg_docker" != "running" ]]; then
      echo -e "  postgres:   ${GREEN}running (local)${NC}"
    fi
  fi
  echo ""

  echo -e "${BOLD}Health:${NC}"
  if curl -sf http://localhost:6060/health &>/dev/null; then
    echo -e "  API:        ${GREEN}healthy${NC} (http://$LAN_IP:6060/health)"
  else
    echo -e "  API:        ${RED}unreachable${NC}"
  fi
  echo ""

  echo -e "${BOLD}Remote Agent Connection:${NC}"
  echo "  From Mac/Windows agents, connect to:"
  echo "    EF__AGENT__CONTROL_PLANE_URL=http://$LAN_IP:6060"
  echo ""
}

do_stop() {
  section "Stopping EdgeFabric"
  stop_service "ef-agent"
  stop_service "ef-api"

  if has docker && docker compose ps --quiet 2>/dev/null | grep -q .; then
    info "Stopping Docker Compose services..."
    docker compose down
  fi
  ok "All services stopped"
}

do_check() {
  section "Device Discovery Check"

  echo -e "${BOLD}Android devices (adb):${NC}"
  if has adb; then
    adb devices -l 2>/dev/null | tail -n +2 | grep -v '^$' || echo "  (none connected)"
  else
    echo "  adb not installed"
  fi
  echo ""

  echo -e "${BOLD}IoT devices (mDNS via avahi-browse):${NC}"
  if has avahi-browse; then
    info "Browsing _edgefabric._tcp for 3 seconds..."
    timeout 3 avahi-browse -t -r -p _edgefabric._tcp 2>/dev/null | grep '^=' | head -10 || echo "  (none found)"
    echo ""
    info "Browsing _mqtt._tcp..."
    timeout 3 avahi-browse -t -r -p _mqtt._tcp 2>/dev/null | grep '^=' | head -10 || echo "  (none found)"
    echo ""
    info "Browsing _http._tcp..."
    timeout 3 avahi-browse -t -r -p _http._tcp 2>/dev/null | grep '^=' | head -10 || echo "  (none found)"
  else
    echo "  avahi-browse not installed (apt install avahi-utils)"
  fi
  echo ""

  echo -e "${BOLD}USB devices:${NC}"
  lsusb 2>/dev/null | grep -iE 'android|google|samsung|pixel|huawei|xiaomi|oneplus|oppo|motorola|sony|lg|htc|raspberry|arduino|esp' || echo "  (no recognized devices)"
  echo ""

  ok "Device check complete"
}

# ══════════════════════════════════════════════════════════════════
# MAIN DISPATCH
# ══════════════════════════════════════════════════════════════════
case "$STEP" in
  install) do_install ;;
  build)   do_build ;;
  deploy)  do_deploy ;;
  status)  do_status ;;
  stop)    do_stop ;;
  check)   do_check ;;
  all)
    do_install
    do_build
    do_deploy
    do_check
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN} EdgeFabric Control Plane is running!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "  Services:"
    echo "    API:        http://$LAN_IP:6060"
    echo "    Swagger UI: http://$LAN_IP:6060/swagger-ui"
    echo "    Health:     http://$LAN_IP:6060/health"
    echo "    NATS:       http://$LAN_IP:8222"
    echo ""
    echo "  Remote Agents (Mac/Windows/other Linux):"
    echo "    EF__AGENT__CONTROL_PLANE_URL=http://$LAN_IP:6060 \\"
    echo "      ./scripts/setup-mac.sh --agent-only"
    echo ""
    echo "  Useful commands:"
    echo "    ./scripts/setup-linux.sh status       # check all services"
    echo "    ./scripts/setup-linux.sh stop         # stop everything"
    echo "    ./scripts/setup-linux.sh check        # scan for devices"
    echo "    tail -f logs/ef-api.log               # watch API logs"
    echo "    tail -f logs/ef-agent.log             # watch agent logs"
    echo ""
    ;;
esac
