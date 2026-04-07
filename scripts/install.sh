#!/usr/bin/env bash
# EdgeFabric — Install Prerequisites
# Supports: macOS (Homebrew) and Linux (apt/dnf)
#
# Usage: ./scripts/install.sh [--skip-docker] [--skip-rust] [--skip-ios] [--skip-android]
#
set -euo pipefail

# ── Colours ───────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ── Parse flags ───────────────────────────────────────────────────
SKIP_DOCKER=false; SKIP_RUST=false; SKIP_IOS=false; SKIP_ANDROID=false; AGENT_ONLY=false
for arg in "$@"; do
  case "$arg" in
    --agent-only)   AGENT_ONLY=true ;;
    --skip-docker)  SKIP_DOCKER=true ;;
    --skip-rust)    SKIP_RUST=true ;;
    --skip-ios)     SKIP_IOS=true ;;
    --skip-android) SKIP_ANDROID=true ;;
    -h|--help)
      echo "Usage: $0 [--agent-only] [--skip-docker] [--skip-rust] [--skip-ios] [--skip-android]"
      echo ""
      echo "  --agent-only   Install only what's needed to run the edge agent"
      echo "                 (Rust, protoc, device tools — skips Docker/infra)"
      exit 0
      ;;
    *) err "Unknown flag: $arg"; exit 1 ;;
  esac
done

# Agent-only mode: skip Docker (infra runs elsewhere)
if [[ "$AGENT_ONLY" == true ]]; then
  SKIP_DOCKER=true
  info "Agent-only mode: skipping Docker/infrastructure dependencies"
fi

# ── Detect OS ─────────────────────────────────────────────────────
OS="$(uname -s)"
case "$OS" in
  Darwin) PLATFORM="macos" ;;
  Linux)  PLATFORM="linux" ;;
  *)      err "Unsupported OS: $OS"; exit 1 ;;
esac
info "Detected platform: $PLATFORM ($(uname -m))"

# ── Package manager helpers ───────────────────────────────────────
pkg_install_mac()   { brew install "$@"; }
pkg_install_linux() {
  if command -v apt-get &>/dev/null; then
    sudo apt-get update -qq && sudo apt-get install -y "$@"
  elif command -v dnf &>/dev/null; then
    sudo dnf install -y "$@"
  elif command -v pacman &>/dev/null; then
    sudo pacman -S --noconfirm "$@"
  else
    err "No supported package manager found (apt/dnf/pacman)"
    exit 1
  fi
}

has() { command -v "$1" &>/dev/null; }

# ── 1. Core build tools ──────────────────────────────────────────
info "Checking core build tools..."

if [[ "$PLATFORM" == "macos" ]]; then
  if ! has brew; then
    info "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  fi
  ok "Homebrew available"

  # Xcode command line tools (needed for C linker, git, etc.)
  if ! xcode-select -p &>/dev/null; then
    info "Installing Xcode Command Line Tools..."
    xcode-select --install
    warn "A dialog may have appeared — please complete the install, then re-run this script."
    exit 0
  fi
  ok "Xcode CLI tools available"
else
  pkg_install_linux build-essential pkg-config curl git
  ok "Core build tools available"
fi

# ── 2. Rust toolchain ────────────────────────────────────────────
if [[ "$SKIP_RUST" == false ]]; then
  info "Checking Rust toolchain..."
  if ! has rustup; then
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  # Ensure minimum version
  RUST_VER="$(rustc --version | awk '{print $2}')"
  info "Rust version: $RUST_VER"

  # Install cargo tools
  if ! has sqlx; then
    info "Installing sqlx-cli..."
    cargo install sqlx-cli --no-default-features --features postgres
  fi
  ok "Rust toolchain ready"
else
  warn "Skipping Rust toolchain (--skip-rust)"
fi

# ── 3. Protobuf compiler ─────────────────────────────────────────
info "Checking protobuf compiler..."
if ! has protoc; then
  if [[ "$PLATFORM" == "macos" ]]; then
    pkg_install_mac protobuf
  else
    pkg_install_linux protobuf-compiler
  fi
fi
ok "protoc $(protoc --version)"

# ── 4. Docker & Docker Compose ────────────────────────────────────
if [[ "$SKIP_DOCKER" == false ]]; then
  info "Checking Docker..."
  if ! has docker; then
    if [[ "$PLATFORM" == "macos" ]]; then
      warn "Docker Desktop not found. Install from https://docker.com/products/docker-desktop"
      warn "Or: brew install --cask docker"
    else
      info "Installing Docker via official script..."
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      warn "You may need to log out and back in for Docker group permissions."
    fi
  fi
  if has docker; then
    ok "Docker $(docker --version | awk '{print $3}' | tr -d ',')"
  fi
else
  warn "Skipping Docker (--skip-docker)"
fi

# ── 5. iOS tools (macOS only) ────────────────────────────────────
if [[ "$PLATFORM" == "macos" && "$SKIP_IOS" == false ]]; then
  info "Checking iOS development tools..."

  if ! has idevice_id; then
    info "Installing libimobiledevice..."
    pkg_install_mac libimobiledevice
  fi
  ok "libimobiledevice available"

  if ! has ideviceinstaller; then
    info "Installing ideviceinstaller..."
    pkg_install_mac ideviceinstaller
  fi
  ok "ideviceinstaller available"

  if ! has ios-deploy; then
    info "Installing ios-deploy..."
    pkg_install_mac ios-deploy
  fi
  ok "ios-deploy available"

  # Check for Xcode (full, not just CLI tools)
  if [[ -d "/Applications/Xcode.app" ]]; then
    XCODE_VER="$(xcodebuild -version 2>/dev/null | head -1 || echo "unknown")"
    ok "Xcode: $XCODE_VER"
  else
    warn "Xcode.app not found — install from the App Store for full iOS testing support."
  fi
elif [[ "$PLATFORM" == "linux" && "$SKIP_IOS" == false ]]; then
  warn "iOS tools are only available on macOS. Skipping."
fi

# ── 6. Android tools ─────────────────────────────────────────────
if [[ "$SKIP_ANDROID" == false ]]; then
  info "Checking Android tools..."
  if ! has adb; then
    if [[ "$PLATFORM" == "macos" ]]; then
      pkg_install_mac android-platform-tools
    else
      pkg_install_linux android-tools-adb 2>/dev/null || pkg_install_linux android-tools
    fi
  fi
  if has adb; then
    ok "adb $(adb version | head -1)"
  fi
else
  warn "Skipping Android tools (--skip-android)"
fi

# ── 7. Networking tools (Linux agent nodes) ───────────────────────
if [[ "$PLATFORM" == "linux" ]]; then
  info "Checking networking tools..."
  if ! has avahi-browse; then
    pkg_install_linux avahi-utils 2>/dev/null || true
  fi
  if ! has usbipd; then
    info "usbipd not found — optional for USB/IP forwarding."
  fi
fi

# ── Summary ───────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN} EdgeFabric prerequisites installed successfully!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo ""
echo "  Next steps:"
if [[ "$AGENT_ONLY" == true ]]; then
echo "    1. ./scripts/build.sh agent             # build the agent binary"
echo "    2. EF__AGENT__CONTROL_PLANE_URL=https://<api-server>:50051 \\"
echo "       ./scripts/deploy.sh agent            # start the agent"
else
echo "    1. ./scripts/build.sh          # build EdgeFabric binaries"
echo "    2. ./scripts/deploy.sh infra   # start Postgres, Redis, NATS"
echo "    3. ./scripts/deploy.sh api     # start the API server"
echo "    4. ./scripts/deploy.sh agent   # start the agent"
fi
echo ""
