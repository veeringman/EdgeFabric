#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DEMO_DIR="$PROJECT_ROOT/demos/EdgeFabricDemo"
PROJECT_PATH="$DEMO_DIR/EdgeFabricDemo.xcodeproj"
SCHEME="EdgeFabricDemo"
BUNDLE_ID="com.edgefabric.demo"

SIMULATOR_UDID="${1:-}"

if ! command -v xcodebuild &>/dev/null; then
  err "xcodebuild not found. Install Xcode and command line tools first."
  exit 1
fi

if ! command -v xcrun &>/dev/null; then
  err "xcrun not found. Install Xcode command line tools first."
  exit 1
fi

if [[ ! -d "$PROJECT_PATH" ]]; then
  err "Demo project not found at $PROJECT_PATH"
  exit 1
fi

if [[ -z "$SIMULATOR_UDID" ]]; then
  SIMULATOR_UDID="$(xcrun simctl list devices booted | sed -n 's/.*(\([A-F0-9-]\{36\}\)) (Booted).*/\1/p' | head -1 | tr -d '[:space:]')"
fi

if [[ -z "$SIMULATOR_UDID" ]]; then
  err "No booted simulator found. Start a simulator or pass a UDID explicitly."
  exit 1
fi

info "Building $SCHEME for simulator $SIMULATOR_UDID"
pushd "$DEMO_DIR" >/dev/null
xcodebuild -project "$PROJECT_PATH" -scheme "$SCHEME" -destination "id=$SIMULATOR_UDID" build
popd >/dev/null

APP_PATH="$(find "$HOME/Library/Developer/Xcode/DerivedData" -path '*/Build/Products/Debug-iphonesimulator/EdgeFabricDemo.app' | head -1)"
if [[ -z "$APP_PATH" ]]; then
  err "Built app bundle not found in DerivedData."
  exit 1
fi

info "Installing app bundle $APP_PATH"
xcrun simctl install "$SIMULATOR_UDID" "$APP_PATH"

info "Launching $BUNDLE_ID"
LAUNCH_OUTPUT="$(xcrun simctl launch "$SIMULATOR_UDID" "$BUNDLE_ID")"
ok "$LAUNCH_OUTPUT"
