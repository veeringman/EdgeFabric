#!/usr/bin/env bash
# EdgeFabric -- macOS One-Shot Setup & Deploy
#
# Tested on: MacBook Pro Early 2015 (Intel i5/i7, 8-16 GB RAM, macOS Monterey)
# Works on any Intel or Apple Silicon Mac with macOS 11+.
#
# Usage:
#   ./scripts/setup-mac.sh                 # install deps + build + deploy all
#   ./scripts/setup-mac.sh --agent-only    # agent node only (API runs elsewhere)
#   ./scripts/setup-mac.sh --step install  # run just the install step
#   ./scripts/setup-mac.sh --step build    # run just the build step
#   ./scripts/setup-mac.sh --step deploy   # run just the deploy step
#   ./scripts/setup-mac.sh --step check    # check what devices are discoverable
#   ./scripts/setup-mac.sh --release       # build in release mode
#
# Environment overrides:
#   EF__AGENT__CONTROL_PLANE_URL=https://api.example.com:6060 ./scripts/setup-mac.sh --agent-only
#   EF__AGENT__AGENT_ID=macbook-lab-01 ./scripts/setup-mac.sh
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
    --step)         : ;; # value handled below
    install|build|deploy|check)
      STEP="$arg" ;;
    -h|--help)
      sed -n '2,19p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) err "Unknown flag: $arg"; exit 1 ;;
  esac
done

# ── Verify macOS ──────────────────────────────────────────────────
if [[ "$(uname)" != "Darwin" ]]; then
  err "This script is for macOS only. Use ./scripts/install.sh on Linux."
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

ARCH="$(uname -m)"              # x86_64 or arm64
MAC_VER="$(sw_vers -productVersion)"
RAM_GB=$(( $(sysctl -n hw.memsize) / 1073741824 ))

echo -e "${BOLD}${CYAN}"
echo "  ╔══════════════════════════════════════════════╗"
echo "  ║     EdgeFabric -- macOS Setup & Deploy       ║"
echo "  ╚══════════════════════════════════════════════╝"
echo -e "${NC}"
info "macOS $MAC_VER ($ARCH), ${RAM_GB} GB RAM"
info "Project root: $PROJECT_ROOT"
echo ""

# ══════════════════════════════════════════════════════════════════
# STEP 1: INSTALL PREREQUISITES
# ══════════════════════════════════════════════════════════════════
do_install() {
  section "1/4  Installing prerequisites"

  # ── Homebrew ────────────────────────────────────────────────────
  if ! command -v brew &>/dev/null; then
    info "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # Add brew to PATH for Apple Silicon
    if [[ "$ARCH" == "arm64" && -f /opt/homebrew/bin/brew ]]; then
      eval "$(/opt/homebrew/bin/brew shellenv)"
    fi
  fi
  ok "Homebrew $(brew --version | head -1 | awk '{print $2}')"

  # ── Xcode CLI tools ────────────────────────────────────────────
  if ! xcode-select -p &>/dev/null; then
    info "Installing Xcode Command Line Tools..."
    xcode-select --install
    warn "Complete the dialog, then re-run this script."
    exit 0
  fi
  ok "Xcode CLI tools: $(xcode-select -p)"

  # ── Rust ────────────────────────────────────────────────────────
  if ! command -v rustup &>/dev/null; then
    info "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
  fi
  ok "Rust $(rustc --version | awk '{print $2}')"

  # ── Protobuf compiler ──────────────────────────────────────────
  if ! command -v protoc &>/dev/null; then
    brew install protobuf
  fi
  ok "protoc $(protoc --version | awk '{print $2}')"

  # ── Docker (infra only, skip for agent-only) ────────────────────
  if [[ "$AGENT_ONLY" == false ]]; then
    if ! command -v docker &>/dev/null; then
      warn "Docker Desktop not installed."
      info "Installing via Homebrew cask..."
      brew install --cask docker
      warn "Open Docker Desktop from Applications and let it start before running deploy."
    else
      ok "Docker $(docker --version | awk '{print $3}' | tr -d ',')"
    fi

    # sqlx-cli for migrations
    if ! command -v sqlx &>/dev/null; then
      info "Installing sqlx-cli..."
      cargo install sqlx-cli --no-default-features --features postgres
    fi
    ok "sqlx-cli available"
  fi

  # ── Android (adb) ──────────────────────────────────────────────
  if ! command -v adb &>/dev/null; then
    info "Installing Android Platform Tools (adb)..."
    brew install --cask android-platform-tools
  fi
  ok "adb $(adb version 2>/dev/null | head -1 | awk '{print $NF}' || echo 'installed')"

  # ── iOS: libimobiledevice ───────────────────────────────────────
  if ! command -v idevice_id &>/dev/null; then
    info "Installing libimobiledevice..."
    brew install libimobiledevice
  fi
  ok "libimobiledevice (idevice_id available)"

  if ! command -v ideviceinstaller &>/dev/null; then
    info "Installing ideviceinstaller..."
    brew install ideviceinstaller
  fi
  ok "ideviceinstaller available"

  if ! command -v ios-deploy &>/dev/null; then
    info "Installing ios-deploy..."
    brew install ios-deploy
  fi
  ok "ios-deploy available"

  echo ""
  ok "All prerequisites installed!"
}

# ══════════════════════════════════════════════════════════════════
# STEP 2: BUILD
# ══════════════════════════════════════════════════════════════════
do_build() {
  section "2/4  Building EdgeFabric"

  # On the Early 2015 MBP with limited RAM, limit parallel jobs
  if [[ $RAM_GB -le 8 ]]; then
    export CARGO_BUILD_JOBS=2
    info "Limiting cargo jobs to 2 (${RAM_GB} GB RAM detected)"
  fi

  BUILD_FLAGS=()
  if [[ "$RELEASE" == true ]]; then
    BUILD_FLAGS+=(--release)
    info "Building in RELEASE mode"
  else
    info "Building in DEBUG mode"
  fi

  if [[ "$AGENT_ONLY" == true ]]; then
    info "Building ef-agent only..."
    cargo build -p ef-agent "${BUILD_FLAGS[@]}"
    ok "ef-agent built"
  else
    info "Building all workspace crates..."
    cargo build "${BUILD_FLAGS[@]}"
    ok "All crates built"
  fi
}

# ══════════════════════════════════════════════════════════════════
# STEP 3: DEPLOY
# ══════════════════════════════════════════════════════════════════
do_deploy() {
  section "3/4  Deploying EdgeFabric"

  if [[ "$AGENT_ONLY" == true ]]; then
    info "Agent-only mode: deploying agent"
    ./scripts/deploy.sh agent
  else
    # Check Docker is running
    if ! docker info &>/dev/null 2>&1; then
      warn "Docker is not running. Starting Docker Desktop..."
      open -a Docker
      info "Waiting for Docker to start (this may take a moment)..."
      local retries=0
      while ! docker info &>/dev/null 2>&1; do
        retries=$((retries + 1))
        if [[ $retries -gt 60 ]]; then
          err "Docker did not start within 60s. Please start Docker Desktop manually."
          exit 1
        fi
        sleep 2
      done
      ok "Docker started"
    fi

    # Run migrations before starting API
    info "Running database migrations..."
    ./scripts/deploy.sh infra
    sleep 3
    DATABASE_URL="postgres://edgefabric:edgefabric@localhost:5432/edgefabric" \
      ./scripts/migrate.sh up

    # Start API
    ./scripts/deploy.sh api

    # Start agent
    ./scripts/deploy.sh agent
  fi
}

# ══════════════════════════════════════════════════════════════════
# STEP 4: DEVICE CHECK
# ══════════════════════════════════════════════════════════════════
do_check() {
  section "4/4  Device Discovery Check"

  echo -e "${BOLD}Android devices (adb):${NC}"
  if command -v adb &>/dev/null; then
    adb devices -l 2>/dev/null | tail -n +2 | grep -v '^$' || echo "  (none connected)"
  else
    echo "  adb not installed"
  fi
  echo ""

  echo -e "${BOLD}iOS real devices (xcrun xctrace):${NC}"
  if command -v xcrun &>/dev/null; then
    xcrun xctrace list devices 2>/dev/null | sed -n '/== Devices ==/,/== Simulators ==/{//!p}' | grep -v '^$' || echo "  (none connected)"
  else
    echo "  xcrun not available"
  fi
  echo ""

  echo -e "${BOLD}iOS real devices (libimobiledevice):${NC}"
  if command -v idevice_id &>/dev/null; then
    local udids
    udids="$(idevice_id -l 2>/dev/null || true)"
    if [[ -n "$udids" ]]; then
      echo "$udids" | while read -r udid; do
        local name
        name="$(ideviceinfo -u "$udid" -k DeviceName 2>/dev/null || echo "unknown")"
        local model
        model="$(ideviceinfo -u "$udid" -k ProductType 2>/dev/null || echo "unknown")"
        local ver
        ver="$(ideviceinfo -u "$udid" -k ProductVersion 2>/dev/null || echo "?")"
        echo "  $name ($model, iOS $ver) -- $udid"
      done
    else
      echo "  (none connected)"
    fi
  else
    echo "  idevice_id not installed"
  fi
  echo ""

  echo -e "${BOLD}iOS/iPadOS simulators (booted):${NC}"
  if command -v xcrun &>/dev/null; then
    xcrun simctl list devices booted 2>/dev/null | grep -v '^\-\-' | grep -v '^$' | grep -v 'unavailable' || echo "  (none booted)"
  else
    echo "  xcrun not available"
  fi
  echo ""

  echo -e "${BOLD}IoT devices (mDNS via dns-sd):${NC}"
  if command -v dns-sd &>/dev/null; then
    info "Browsing _edgefabric._tcp for 3 seconds..."
    timeout 3 dns-sd -B _edgefabric._tcp 2>/dev/null | tail -n +5 || echo "  (none found)"
    echo ""
    info "Browsing _mqtt._tcp for 3 seconds..."
    timeout 3 dns-sd -B _mqtt._tcp 2>/dev/null | tail -n +5 || echo "  (none found)"
    echo ""
    info "Browsing _http._tcp for 3 seconds..."
    timeout 3 dns-sd -B _http._tcp 2>/dev/null | tail -n +5 || echo "  (none found)"
  else
    echo "  dns-sd not available (should be built-in on macOS)"
  fi
  echo ""

  echo -e "${BOLD}USB devices:${NC}"
  system_profiler SPUSBDataType 2>/dev/null | grep -E '^\s+(iPhone|iPad|Android|Pixel|Samsung|Google|Raspberry|Arduino|ESP)' || echo "  (no recognized devices)"
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
  check)   do_check ;;
  all)
    do_install
    do_build
    do_deploy
    do_check
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN} EdgeFabric is running on your Mac!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "  Services:"
    echo "    API:        http://127.0.0.1:6060"
    echo "    Swagger UI: http://127.0.0.1:6060/swagger-ui"
    echo "    Health:     http://127.0.0.1:6060/health"
    echo ""
    echo "  Useful commands:"
    echo "    ./scripts/deploy.sh status           # check all services"
    echo "    ./scripts/deploy.sh stop             # stop everything"
    echo "    ./scripts/setup-mac.sh check         # re-scan devices"
    echo "    tail -f logs/ef-agent.log            # watch agent logs"
    echo "    tail -f logs/ef-api.log              # watch API logs"
    echo ""
    if [[ "$AGENT_ONLY" == true ]]; then
      echo "  Agent-only mode -- API: ${EF__AGENT__CONTROL_PLANE_URL:-http://localhost:6060}"
    fi
    ;;
esac
