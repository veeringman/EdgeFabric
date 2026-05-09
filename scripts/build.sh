#!/usr/bin/env bash
# EdgeFabric — Build Script (macOS & Linux)
#
# Usage:
#   ./scripts/build.sh              # build all (debug)
#   ./scripts/build.sh --release    # build all (release, optimised)
#   ./scripts/build.sh api          # build only ef-api
#   ./scripts/build.sh agent        # build only ef-agent
#   ./scripts/build.sh cli          # build only ef (CLI)
#   ./scripts/build.sh dashboard    # build WASM dashboard via trunk
#   ./scripts/build.sh --release api agent   # release build specific targets
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

# ── Parse arguments ───────────────────────────────────────────────
RELEASE=false
TARGETS=()
for arg in "$@"; do
  case "$arg" in
    --release|-r) RELEASE=true ;;
    -h|--help)
      echo "Usage: $0 [--release] [api|agent|cli|dashboard|all]"
      exit 0
      ;;
    *) TARGETS+=("$arg") ;;
  esac
done

# Default: build all workspace crates
if [[ ${#TARGETS[@]} -eq 0 ]]; then
  TARGETS=("all")
fi

CARGO_FLAGS=()
if [[ "$RELEASE" == true ]]; then
  CARGO_FLAGS+=(--release)
  BUILD_DIR="target/release"
  info "Building in RELEASE mode"
else
  BUILD_DIR="target/debug"
  info "Building in DEBUG mode"
fi

# ── Pre-flight checks ────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  err "Rust/cargo not found. Run: ./scripts/install.sh"
  exit 1
fi

# protoc is only needed for crates that compile .proto files (ef-api, or 'all')
NEEDS_PROTOC=false
for target in "${TARGETS[@]}"; do
  case "$target" in
    all|api) NEEDS_PROTOC=true ;;
  esac
done

if [[ "$NEEDS_PROTOC" == true ]] && ! command -v protoc &>/dev/null; then
  if [[ "$(uname)" == "Darwin" ]] && command -v brew &>/dev/null; then
    info "protoc not found — installing via Homebrew..."
    brew install protobuf
  elif command -v apt-get &>/dev/null; then
    info "protoc not found — installing via apt..."
    sudo apt-get update -qq && sudo apt-get install -y protobuf-compiler
  else
    err "protoc not found. Run: ./scripts/install.sh"
    exit 1
  fi
fi

# ── macOS: check device discovery tooling ─────────────────────────
if [[ "$(uname)" == "Darwin" ]]; then
  info "macOS detected -- checking device discovery dependencies"

  # Android (adb)
  if command -v adb &>/dev/null; then
    ok "adb found (Android discovery ready)"
  else
    warn "adb not found -- Android device discovery will be unavailable"
    warn "  Install: brew install --cask android-platform-tools"
  fi

  # iOS real devices (xcrun is part of Xcode CLI tools)
  if command -v xcrun &>/dev/null; then
    ok "xcrun found (iOS/iPadOS discovery ready)"
    # Check Xcode CLI tools are fully installed
    if ! xcode-select -p &>/dev/null; then
      warn "Xcode CLI tools not fully configured: xcode-select --install"
    fi
  else
    warn "xcrun not found -- iOS device discovery will be unavailable"
    warn "  Install: xcode-select --install"
  fi

  # iOS real devices via USB (libimobiledevice)
  if command -v idevice_id &>/dev/null; then
    ok "libimobiledevice found (USB iOS discovery ready)"
  else
    warn "idevice_id not found -- USB iOS fallback unavailable"
    warn "  Install: brew install libimobiledevice"
  fi

  # IoT (dns-sd is built-in on macOS, but verify)
  if command -v dns-sd &>/dev/null; then
    ok "dns-sd found (IoT mDNS discovery ready)"
  else
    warn "dns-sd not found -- IoT discovery will be unavailable"
  fi
fi

# ── Build functions ───────────────────────────────────────────────
build_crate() {
  local crate="$1"
  info "Building $crate..."
  if [[ ${#CARGO_FLAGS[@]} -gt 0 ]]; then
    cargo build -p "$crate" "${CARGO_FLAGS[@]}"
  else
    cargo build -p "$crate"
  fi
  ok "$crate built → $BUILD_DIR/"
}

build_dashboard() {
  info "Building ef-dashboard (WASM)..."
  if ! command -v trunk &>/dev/null; then
    info "Installing trunk..."
    cargo install trunk
  fi
  if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
    info "Adding wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
  fi
  pushd crates/ef-dashboard >/dev/null
  trunk build ${RELEASE:+--release}
  popd >/dev/null
  ok "ef-dashboard built"
}

build_all() {
  info "Building all workspace crates..."
  if [[ ${#CARGO_FLAGS[@]} -gt 0 ]]; then
    cargo build "${CARGO_FLAGS[@]}"
  else
    cargo build
  fi
  ok "All crates built → $BUILD_DIR/"
}

# ── Execute builds ────────────────────────────────────────────────
START_TIME=$SECONDS

for target in "${TARGETS[@]}"; do
  case "$target" in
    all)        build_all ;;
    api)        build_crate ef-api ;;
    agent)      build_crate ef-agent ;;
    cli)        build_crate ef-cli ;;
    dashboard)  build_dashboard ;;
    *)          err "Unknown target: $target (use: all|api|agent|cli|dashboard)"; exit 1 ;;
  esac
done

ELAPSED=$((SECONDS - START_TIME))

# ── Summary ───────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN} Build completed in ${ELAPSED}s${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"

if [[ "$RELEASE" == true ]]; then
  echo ""
  echo "  Binaries:"
  for bin in ef-api ef-agent ef; do
    if [[ -f "$BUILD_DIR/$bin" ]]; then
      SIZE=$(du -h "$BUILD_DIR/$bin" | awk '{print $1}')
      echo "    $BUILD_DIR/$bin  ($SIZE)"
    fi
  done
fi
echo ""
