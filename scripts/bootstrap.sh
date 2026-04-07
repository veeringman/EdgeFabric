#!/usr/bin/env bash
# EdgeFabric — Bootstrap Installer
#
# Run directly from the internet:
#   curl -fsSL https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/veeringman/EdgeFabric/main/scripts/bootstrap.sh | bash -s -- --agent-only
#
# What it does:
#   1. Ensures git is installed
#   2. Clones the EdgeFabric repo (or pulls if it already exists)
#   3. Runs scripts/install.sh with any flags you passed
#
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

REPO_URL="https://github.com/veeringman/EdgeFabric.git"
INSTALL_DIR="${EDGEFABRIC_DIR:-$HOME/edgefabric}"
BRANCH="${EDGEFABRIC_BRANCH:-main}"

echo ""
echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║       EdgeFabric Bootstrap Installer     ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"
echo ""

# ── 1. Ensure git ────────────────────────────────────────────────
if ! command -v git &>/dev/null; then
  info "Git not found — installing..."
  OS="$(uname -s)"
  if [[ "$OS" == "Darwin" ]]; then
    xcode-select --install 2>/dev/null || true
    echo "Please complete Xcode CLI tools install, then re-run this script."
    exit 0
  else
    if command -v apt-get &>/dev/null; then
      sudo apt-get update -qq && sudo apt-get install -y git
    elif command -v dnf &>/dev/null; then
      sudo dnf install -y git
    elif command -v pacman &>/dev/null; then
      sudo pacman -S --noconfirm git
    else
      err "Cannot install git automatically. Please install git and re-run."
      exit 1
    fi
  fi
fi
ok "git $(git --version | awk '{print $3}')"

# ── 2. Clone or update repo ─────────────────────────────────────
if [[ -d "$INSTALL_DIR/.git" ]]; then
  info "EdgeFabric repo found at $INSTALL_DIR — pulling latest..."
  git -C "$INSTALL_DIR" fetch origin "$BRANCH"
  git -C "$INSTALL_DIR" checkout "$BRANCH"
  git -C "$INSTALL_DIR" pull --ff-only origin "$BRANCH"
  ok "Repository updated"
else
  info "Cloning EdgeFabric to $INSTALL_DIR ..."
  git clone --branch "$BRANCH" "$REPO_URL" "$INSTALL_DIR"
  ok "Repository cloned"
fi

# ── 3. Run the full install script ───────────────────────────────
info "Running install script..."
echo ""
chmod +x "$INSTALL_DIR/scripts/install.sh"
exec "$INSTALL_DIR/scripts/install.sh" "$@"
