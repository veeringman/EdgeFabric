#!/usr/bin/env bash
# EdgeFabric -- WebDriverAgent (WDA) manager for macOS
#
# Manages WDA lifecycle for iOS real devices:
# - prerequisite checks
# - optional clone/bootstrap of WDA sources
# - xcodebuild launch
# - iproxy tunnel (localhost:<port> -> device:8100)
# - status/logs/stop helpers

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

if [[ "$(uname)" != "Darwin" ]]; then
  err "WDA manager is macOS-only."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

PID_DIR="$PROJECT_ROOT/.pids"
LOG_DIR="$PROJECT_ROOT/logs"
mkdir -p "$PID_DIR" "$LOG_DIR"

WDA_XCODE_PID_FILE="$PID_DIR/wda-xcodebuild.pid"
WDA_IPROXY_PID_FILE="$PID_DIR/wda-iproxy.pid"
WDA_XCODE_LOG="$LOG_DIR/wda-xcodebuild.log"
WDA_IPROXY_LOG="$LOG_DIR/wda-iproxy.log"

# Defaults can be overridden via env or CLI options
DEFAULT_WDA_PATH="${WDA_PATH:-}"
DEFAULT_WDA_PORT="${WDA_LOCAL_PORT:-8100}"
DEFAULT_WDA_DERIVED_DATA="${WDA_DERIVED_DATA:-$PROJECT_ROOT/.build/wda-derived-data}"
DEFAULT_WDA_REPO_URL="${WDA_REPO_URL:-https://github.com/appium/WebDriverAgent.git}"

usage() {
  cat <<'EOF'
EdgeFabric WDA manager (macOS)

Usage:
  ./scripts/wda-manage.sh <command> [options]

Commands:
  doctor                      Check WDA prerequisites and runtime state
  init                        Clone WDA source repo if missing
  start                       Build + run WDA and start iproxy
  stop                        Stop WDA + iproxy processes
  restart                     Stop then start WDA
  status                      Show process + endpoint status
  logs                        Show WDA logs

Common options:
  --udid <udid>               iOS device UDID (auto-detect if omitted)
  --wda-path <path>           Path to WebDriverAgent source
  --port <port>               Local tunnel port (default: 8100)

Start-specific options:
  --team-id <team_id>         Apple Developer team ID (for signing)
  --bundle-id <bundle_id>     Bundle ID override for WDA runner
  --derived-data <path>       DerivedData directory

Logs options:
  -f, --follow                Follow logs output

Examples:
  ./scripts/wda-manage.sh doctor
  ./scripts/wda-manage.sh init --wda-path "$HOME/WebDriverAgent"
  ./scripts/wda-manage.sh start --udid <UDID> --team-id ABCD123456 --wda-path "$HOME/WebDriverAgent"
  ./scripts/wda-manage.sh status --port 8100
EOF
}

read_pid() {
  local pid_file="$1"
  if [[ -f "$pid_file" ]]; then
    cat "$pid_file"
  fi
}

save_pid() {
  local pid_file="$1" pid="$2"
  echo "$pid" > "$pid_file"
}

kill_pid_file() {
  local pid_file="$1" name="$2"
  local pid
  pid="$(read_pid "$pid_file")"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    info "Stopping $name (PID $pid)..."
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 10); do
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
      warn "Force stopping $name..."
      kill -9 "$pid" 2>/dev/null || true
    fi
    ok "$name stopped"
  fi
  rm -f "$pid_file"
}

is_full_xcode_ready() {
  xcodebuild -version >/dev/null 2>&1
}

detect_udid() {
  # 1) Prefer connected real devices via libimobiledevice.
  if command -v idevice_id >/dev/null 2>&1; then
    local real_udid
    real_udid="$(idevice_id -l 2>/dev/null | head -n1 | tr -d '[:space:]')"
    if [[ -n "$real_udid" ]]; then
      echo "$real_udid"
      return 0
    fi
  fi

  # 2) Fallback: parse xctrace list (excluding simulators).
  if command -v xcrun >/dev/null 2>&1; then
    local xctrace_udid
    xctrace_udid="$(xcrun xctrace list devices 2>/dev/null \
      | awk -F '[()]' '/\(.*\)/ && $0 !~ /Simulator/ {print $2}' \
      | grep -E '^[A-Fa-f0-9-]{20,}$' \
      | head -n1 || true)"
    if [[ -n "$xctrace_udid" ]]; then
      echo "$xctrace_udid"
      return 0
    fi
  fi

  return 1
}

resolve_wda_path() {
  local requested="$1"

  if [[ -n "$requested" ]]; then
    echo "$requested"
    return 0
  fi

  if [[ -n "$DEFAULT_WDA_PATH" ]]; then
    echo "$DEFAULT_WDA_PATH"
    return 0
  fi

  for candidate in \
    "$HOME/WebDriverAgent" \
    "$PROJECT_ROOT/vendor/WebDriverAgent" \
    "$PROJECT_ROOT/third_party/WebDriverAgent"
  do
    if [[ -d "$candidate" ]]; then
      echo "$candidate"
      return 0
    fi
  done

  # Default suggestion when missing.
  echo "$HOME/WebDriverAgent"
}

validate_wda_project() {
  local wda_path="$1"
  [[ -f "$wda_path/WebDriverAgent.xcodeproj/project.pbxproj" ]]
}

wait_for_wda_status() {
  local port="$1" timeout="${2:-120}"
  for _ in $(seq 1 "$timeout"); do
    if curl -sf "http://127.0.0.1:${port}/status" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

run_doctor() {
  local udid="$1"
  local wda_path="$2"
  local port="$3"

  echo -e "${BOLD}WDA doctor${NC}"
  echo "Project: $PROJECT_ROOT"
  echo "WDA path: $wda_path"
  echo ""

  if is_full_xcode_ready; then
    ok "xcodebuild: $(xcodebuild -version | tr '\n' ' ' | sed 's/  */ /g')"
  else
    err "xcodebuild: full Xcode is not active"
    warn "Install Xcode.app and switch developer dir (needs admin):"
    warn "  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer"
  fi

  if command -v xcrun >/dev/null 2>&1; then
    ok "xcrun: available"
  else
    err "xcrun: not found"
  fi

  if command -v idevice_id >/dev/null 2>&1; then
    ok "idevice_id: available"
  else
    warn "idevice_id: not found (install libimobiledevice)"
  fi

  if command -v iproxy >/dev/null 2>&1; then
    ok "iproxy: available"
  else
    warn "iproxy: not found (install libimobiledevice/usbmuxd tools)"
  fi

  if command -v ffmpeg >/dev/null 2>&1; then
    ok "ffmpeg: available"
  else
    warn "ffmpeg: missing (needed by some iOS capture paths)"
  fi

  if validate_wda_project "$wda_path"; then
    ok "WDA source: found"
  else
    warn "WDA source: not found at $wda_path"
    warn "Run: ./scripts/wda-manage.sh init --wda-path \"$wda_path\""
  fi

  if [[ -z "$udid" ]]; then
    udid="$(detect_udid || true)"
  fi
  if [[ -n "$udid" ]]; then
    ok "Device UDID: $udid"
  else
    warn "No connected iOS device detected"
  fi

  if curl -sf "http://127.0.0.1:${port}/status" >/dev/null 2>&1; then
    ok "WDA endpoint: http://127.0.0.1:${port}/status is reachable"
  else
    warn "WDA endpoint: not reachable on port ${port}"
  fi

  echo ""
  run_status "$port"
}

run_init() {
  local wda_path="$1"

  if validate_wda_project "$wda_path"; then
    ok "WDA already present at $wda_path"
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    err "git is required to clone WDA"
    exit 1
  fi

  info "Cloning WebDriverAgent into $wda_path ..."
  mkdir -p "$(dirname "$wda_path")"
  git clone "$DEFAULT_WDA_REPO_URL" "$wda_path"

  if validate_wda_project "$wda_path"; then
    ok "WDA cloned successfully"
  else
    err "WDA clone completed but project file not found"
    exit 1
  fi
}

run_stop() {
  kill_pid_file "$WDA_IPROXY_PID_FILE" "iproxy"
  kill_pid_file "$WDA_XCODE_PID_FILE" "xcodebuild/WDA"
}

run_status() {
  local port="$1"

  echo -e "${BOLD}WDA status${NC}"

  local xpid
  xpid="$(read_pid "$WDA_XCODE_PID_FILE")"
  if [[ -n "$xpid" ]] && kill -0 "$xpid" 2>/dev/null; then
    echo -e "  xcodebuild: ${GREEN}running${NC} (PID $xpid)"
  else
    echo -e "  xcodebuild: ${RED}stopped${NC}"
  fi

  local ipid
  ipid="$(read_pid "$WDA_IPROXY_PID_FILE")"
  if [[ -n "$ipid" ]] && kill -0 "$ipid" 2>/dev/null; then
    echo -e "  iproxy:     ${GREEN}running${NC} (PID $ipid)"
  else
    echo -e "  iproxy:     ${RED}stopped${NC}"
  fi

  if curl -sf "http://127.0.0.1:${port}/status" >/dev/null 2>&1; then
    echo -e "  endpoint:   ${GREEN}ready${NC} (http://127.0.0.1:${port}/status)"
  else
    echo -e "  endpoint:   ${YELLOW}not ready${NC} (http://127.0.0.1:${port}/status)"
  fi
}

run_logs() {
  local follow="$1"

  if [[ ! -f "$WDA_XCODE_LOG" && ! -f "$WDA_IPROXY_LOG" ]]; then
    warn "No WDA logs yet"
    return 0
  fi

  local cmd=(tail)
  if [[ "$follow" == "true" ]]; then
    cmd+=("-f")
  else
    cmd+=("-n" "80")
  fi

  [[ -f "$WDA_XCODE_LOG" ]] && cmd+=("$WDA_XCODE_LOG")
  [[ -f "$WDA_IPROXY_LOG" ]] && cmd+=("$WDA_IPROXY_LOG")
  "${cmd[@]}"
}

run_start() {
  local udid="$1"
  local wda_path="$2"
  local port="$3"
  local team_id="$4"
  local bundle_id="$5"
  local derived_data="$6"

  if ! is_full_xcode_ready; then
    err "Full Xcode is not active. Cannot build WDA."
    warn "Install Xcode.app and run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer"
    exit 1
  fi

  if ! validate_wda_project "$wda_path"; then
    err "WDA source not found at $wda_path"
    warn "Run: ./scripts/wda-manage.sh init --wda-path \"$wda_path\""
    exit 1
  fi

  if [[ -z "$udid" ]]; then
    udid="$(detect_udid || true)"
  fi
  if [[ -z "$udid" ]]; then
    err "No iOS device UDID provided/detected"
    exit 1
  fi

  run_stop || true

  mkdir -p "$(dirname "$derived_data")"
  : > "$WDA_XCODE_LOG"
  : > "$WDA_IPROXY_LOG"

  local xcode_cmd=(
    xcodebuild
    -project "$wda_path/WebDriverAgent.xcodeproj"
    -scheme WebDriverAgentRunner
    -destination "id=$udid"
    -configuration Debug
    -derivedDataPath "$derived_data"
    -allowProvisioningUpdates
    test
  )

  if [[ -n "$team_id" ]]; then
    xcode_cmd+=(DEVELOPMENT_TEAM="$team_id")
    xcode_cmd+=(CODE_SIGN_STYLE=Automatic)
  else
    warn "No --team-id provided. Build may fail if signing is not configured in Xcode."
  fi

  if [[ -n "$bundle_id" ]]; then
    xcode_cmd+=(PRODUCT_BUNDLE_IDENTIFIER="$bundle_id")
  fi

  info "Starting WDA xcodebuild for UDID $udid"
  (
    cd "$wda_path"
    nohup "${xcode_cmd[@]}" > "$WDA_XCODE_LOG" 2>&1 &
    echo $! > "$WDA_XCODE_PID_FILE"
  )

  if command -v iproxy >/dev/null 2>&1; then
    info "Starting iproxy tunnel 127.0.0.1:${port} -> device:8100"
    nohup iproxy "$port" 8100 -u "$udid" > "$WDA_IPROXY_LOG" 2>&1 &
    save_pid "$WDA_IPROXY_PID_FILE" "$!"
  else
    warn "iproxy not found; endpoint forwarding may fail"
  fi

  info "Waiting for WDA endpoint..."
  if wait_for_wda_status "$port" 120; then
    ok "WDA is ready at http://127.0.0.1:${port}/status"
  else
    warn "WDA endpoint did not become ready within timeout"
    warn "Check logs: $WDA_XCODE_LOG"
    warn "Tail logs: ./scripts/wda-manage.sh logs -f"
    return 1
  fi

  run_status "$port"
}

command="${1:-help}"
shift || true

udid=""
wda_path=""
port="$DEFAULT_WDA_PORT"
team_id="${WDA_TEAM_ID:-}"
bundle_id="${WDA_BUNDLE_ID:-}"
derived_data="$DEFAULT_WDA_DERIVED_DATA"
follow_logs="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --udid)
      [[ $# -ge 2 ]] || { err "Missing value for --udid"; exit 1; }
      udid="$2"
      shift 2
      ;;
    --wda-path)
      [[ $# -ge 2 ]] || { err "Missing value for --wda-path"; exit 1; }
      wda_path="$2"
      shift 2
      ;;
    --port)
      [[ $# -ge 2 ]] || { err "Missing value for --port"; exit 1; }
      port="$2"
      shift 2
      ;;
    --team-id)
      [[ $# -ge 2 ]] || { err "Missing value for --team-id"; exit 1; }
      team_id="$2"
      shift 2
      ;;
    --bundle-id)
      [[ $# -ge 2 ]] || { err "Missing value for --bundle-id"; exit 1; }
      bundle_id="$2"
      shift 2
      ;;
    --derived-data)
      [[ $# -ge 2 ]] || { err "Missing value for --derived-data"; exit 1; }
      derived_data="$2"
      shift 2
      ;;
    -f|--follow)
      follow_logs="true"
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

wda_path="$(resolve_wda_path "$wda_path")"

case "$command" in
  doctor)
    run_doctor "$udid" "$wda_path" "$port"
    ;;
  init)
    run_init "$wda_path"
    ;;
  start)
    run_start "$udid" "$wda_path" "$port" "$team_id" "$bundle_id" "$derived_data"
    ;;
  stop)
    run_stop
    ;;
  restart)
    run_stop
    run_start "$udid" "$wda_path" "$port" "$team_id" "$bundle_id" "$derived_data"
    ;;
  status)
    run_status "$port"
    ;;
  logs)
    run_logs "$follow_logs"
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
