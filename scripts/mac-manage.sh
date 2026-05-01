#!/usr/bin/env bash
# EdgeFabric -- macOS service manager
#
# A thin wrapper around existing project scripts to simplify
# day-to-day local operations on macOS.
#
# Usage examples:
#   ./scripts/mac-manage.sh doctor
#   ./scripts/mac-manage.sh build
#   ./scripts/mac-manage.sh start agent --control-plane-url http://192.168.29.78:6060
#   ./scripts/mac-manage.sh start local
#   ./scripts/mac-manage.sh logs agent -f
#   ./scripts/mac-manage.sh stop
#   ./scripts/mac-manage.sh status

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

if [[ "$(uname)" != "Darwin" ]]; then
  err "This script is intended for macOS."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

DEPLOY_SCRIPT="$PROJECT_ROOT/scripts/deploy.sh"
BUILD_SCRIPT="$PROJECT_ROOT/scripts/build.sh"
WDA_SCRIPT="$PROJECT_ROOT/scripts/wda-manage.sh"

if [[ ! -x "$DEPLOY_SCRIPT" ]]; then
  err "Missing executable: $DEPLOY_SCRIPT"
  exit 1
fi
if [[ ! -x "$BUILD_SCRIPT" ]]; then
  err "Missing executable: $BUILD_SCRIPT"
  exit 1
fi

ensure_rust_env() {
  if [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
}

ensure_local_protoc() {
  # Prefer user-local protoc if present (useful when brew is unavailable).
  if [[ -z "${PROTOC:-}" && -x "$HOME/.local/bin/bin/protoc" ]]; then
    export PROTOC="$HOME/.local/bin/bin/protoc"
  fi
}

usage() {
  cat <<'EOF'
EdgeFabric macOS manager

Usage:
  ./scripts/mac-manage.sh <command> [target] [options]

Commands:
  doctor                     Show prerequisite/tooling checks
  build [target]             Build binaries (default target: agent)
  start [target]             Start services
  stop                       Stop all services managed by deploy.sh
  restart [target]           Restart services (stop + start)
  status                     Show service status
  logs [target]              Show logs (default target: all)
  wda <subcommand>           Run WDA manager (doctor/init/start/stop/status/logs)

Targets:
  agent                      Agent only
  api                        API (+ infra via deploy.sh api)
  local                      Infra + API + Agent + Dashboard (deploy.sh all)
  all                        Same as local

Options:
  --control-plane-url <url>  Set EF__AGENT__CONTROL_PLANE_URL for agent start
  --agent-id <id>            Set EF__AGENT__AGENT_ID
  --region <region>          Set EF__AGENT__REGION
  --run-mode <mode>          Set EF_RUN_MODE (default: development)
  -f, --follow               Follow logs output
  -h, --help                 Show this help

Examples:
  ./scripts/mac-manage.sh build agent
  ./scripts/mac-manage.sh start agent --control-plane-url http://192.168.29.78:6060
  ./scripts/mac-manage.sh start api
  ./scripts/mac-manage.sh logs agent -f
EOF
}

command="${1:-help}"
shift || true

if [[ "$command" == "wda" ]]; then
  if [[ ! -x "$WDA_SCRIPT" ]]; then
    err "Missing executable: $WDA_SCRIPT"
    exit 1
  fi
  exec "$WDA_SCRIPT" "$@"
fi

target=""
control_plane_url=""
agent_id=""
region=""
run_mode="development"
follow_logs=false

# Optional positional target (if next arg is not a flag).
if [[ $# -gt 0 && "${1:0:1}" != "-" ]]; then
  target="$1"
  shift
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --control-plane-url)
      [[ $# -ge 2 ]] || { err "Missing value for --control-plane-url"; exit 1; }
      control_plane_url="$2"
      shift 2
      ;;
    --agent-id)
      [[ $# -ge 2 ]] || { err "Missing value for --agent-id"; exit 1; }
      agent_id="$2"
      shift 2
      ;;
    --region)
      [[ $# -ge 2 ]] || { err "Missing value for --region"; exit 1; }
      region="$2"
      shift 2
      ;;
    --run-mode)
      [[ $# -ge 2 ]] || { err "Missing value for --run-mode"; exit 1; }
      run_mode="$2"
      shift 2
      ;;
    -f|--follow)
      follow_logs=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      err "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

if [[ -n "$control_plane_url" ]]; then export EF__AGENT__CONTROL_PLANE_URL="$control_plane_url"; fi
if [[ -n "$agent_id" ]]; then export EF__AGENT__AGENT_ID="$agent_id"; fi
if [[ -n "$region" ]]; then export EF__AGENT__REGION="$region"; fi
export EF_RUN_MODE="$run_mode"

ensure_rust_env
ensure_local_protoc

show_doctor() {
  echo -e "${BOLD}EdgeFabric macOS doctor${NC}"
  echo "Project: $PROJECT_ROOT"
  echo ""

  if command -v cargo >/dev/null 2>&1; then
    ok "cargo: $(cargo --version | head -1)"
  else
    err "cargo: not found"
  fi

  if [[ -n "${PROTOC:-}" ]] && [[ -x "$PROTOC" ]]; then
    ok "protoc: $PROTOC ($($PROTOC --version 2>/dev/null || echo unknown))"
  elif command -v protoc >/dev/null 2>&1; then
    ok "protoc: $(protoc --version)"
  else
    warn "protoc: not found (needed for ef-api build)"
  fi

  if command -v docker >/dev/null 2>&1; then
    ok "docker: $(docker --version | head -1)"
  else
    warn "docker: not found (required for local infra/api)"
  fi

  if command -v ffmpeg >/dev/null 2>&1; then
    ok "ffmpeg: $(ffmpeg -version | head -1)"
  else
    warn "ffmpeg: not found (needed for some screen capture paths)"
  fi

  echo ""
  "$DEPLOY_SCRIPT" status || true
}

run_build() {
  local build_target="${target:-agent}"
  case "$build_target" in
    agent)
      info "Building ef-agent (release)..."
      "$BUILD_SCRIPT" --release agent
      ;;
    api)
      info "Building ef-api (release)..."
      "$BUILD_SCRIPT" --release api
      ;;
    local|all)
      info "Building ef-api + ef-agent (release)..."
      "$BUILD_SCRIPT" --release api agent
      ;;
    *)
      err "Invalid build target: $build_target"
      exit 1
      ;;
  esac
}

run_start() {
  local start_target="${target:-agent}"
  case "$start_target" in
    agent)
      info "Starting agent..."
      [[ -n "${EF__AGENT__CONTROL_PLANE_URL:-}" ]] && info "Control plane: ${EF__AGENT__CONTROL_PLANE_URL}"
      "$DEPLOY_SCRIPT" agent
      ;;
    api)
      info "Starting infra + API..."
      "$DEPLOY_SCRIPT" api
      ;;
    local|all)
      info "Starting full local stack..."
      "$DEPLOY_SCRIPT" all
      ;;
    *)
      err "Invalid start target: $start_target"
      exit 1
      ;;
  esac
}

run_logs() {
  local logs_target="${target:-all}"
  local follow_arg=""
  if [[ "$follow_logs" == true ]]; then
    follow_arg="-f"
  fi

  case "$logs_target" in
    agent)
      [[ -f "$PROJECT_ROOT/logs/ef-agent.log" ]] || { warn "No logs/ef-agent.log yet"; exit 0; }
      tail $follow_arg "$PROJECT_ROOT/logs/ef-agent.log"
      ;;
    api)
      [[ -f "$PROJECT_ROOT/logs/ef-api.log" ]] || { warn "No logs/ef-api.log yet"; exit 0; }
      tail $follow_arg "$PROJECT_ROOT/logs/ef-api.log"
      ;;
    dashboard)
      [[ -f "$PROJECT_ROOT/logs/ef-dashboard.log" ]] || { warn "No logs/ef-dashboard.log yet"; exit 0; }
      tail $follow_arg "$PROJECT_ROOT/logs/ef-dashboard.log"
      ;;
    all)
      local cmd=(tail)
      [[ -n "$follow_arg" ]] && cmd+=("$follow_arg")
      cmd+=(
        "$PROJECT_ROOT/logs/ef-api.log"
        "$PROJECT_ROOT/logs/ef-agent.log"
      )
      "${cmd[@]}" 2>/dev/null || warn "No logs yet. Start services first."
      ;;
    *)
      err "Invalid logs target: $logs_target"
      exit 1
      ;;
  esac
}

case "$command" in
  doctor)
    show_doctor
    ;;
  build)
    run_build
    ;;
  start)
    run_start
    ;;
  stop)
    info "Stopping services..."
    "$DEPLOY_SCRIPT" stop
    ;;
  restart)
    info "Restarting services..."
    "$DEPLOY_SCRIPT" stop
    run_start
    ;;
  status)
    "$DEPLOY_SCRIPT" status
    ;;
  logs)
    run_logs
    ;;
  help|-h|--help|"")
    usage
    ;;
  *)
    err "Unknown command: $command"
    usage
    exit 1
    ;;
esac
