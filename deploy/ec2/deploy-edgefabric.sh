#!/usr/bin/env bash
# Deploy EdgeFabric API (and optional agent) to the eGenie R&D EC2 host.
#
# Usage:
#   cp deploy/ec2/host.env.example deploy/ec2/host.env   # once
#   ./deploy/ec2/deploy-edgefabric.sh
#
# Same host/key as eGenie by default (100.52.147.238, VeerSetuHost.pem).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=linux-build.sh
source "$SCRIPT_DIR/linux-build.sh"

if [[ -f "$SCRIPT_DIR/host.env" ]]; then
  # shellcheck disable=SC1091
  source "$SCRIPT_DIR/host.env"
fi

HOST="${DEPLOY_HOST:-100.52.147.238}"
KEY="${DEPLOY_KEY:-/Users/vijay/rnd/projects/VeerSetuHost.pem}"
USER="${DEPLOY_USER:-ubuntu}"
PUBLIC_URL="${EF_PUBLIC_URL:-https://edgefabric.veerlabs.solutions}"
EDGEFABRIC_ROOT="${EDGEFABRIC_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
DEPLOY_AGENT="${DEPLOY_AGENT:-0}"

linux_build_init "$HOST" "$KEY" "$USER"
SSH="$LINUX_BUILD_SSH"

if [[ ! -f "$KEY" ]]; then
  echo "SSH key not found: $KEY" >&2
  echo "Copy deploy/ec2/host.env.example → host.env and set DEPLOY_KEY." >&2
  exit 1
fi

BUILD_MODE_RESOLVED="$(linux_build_resolve_mode)"
echo "==> EdgeFabric deploy to $HOST (build mode: $BUILD_MODE_RESOLVED)"

CARGO_BUILD_CMD="cargo build --release -p ef-api"
if [[ "$DEPLOY_AGENT" == "1" ]]; then
  CARGO_BUILD_CMD="$CARGO_BUILD_CMD -p ef-agent"
fi

SKIP_EC2_BUILD=0
if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
  SKIP_EC2_BUILD=1
elif [[ "$BUILD_MODE_RESOLVED" == "lan" ]]; then
  linux_build_on_lan "$EDGEFABRIC_ROOT" "$CARGO_BUILD_CMD" "$(linux_build_lan_cargo_target)"
  if ! linux_build_ec2_reachable; then
    linux_build_lan_only_hint
    exit 0
  fi
  SKIP_EC2_BUILD=1
fi

EF_EC2_TARGET="$(linux_build_ec2_cargo_target)"

if ! linux_build_ec2_reachable; then
  echo "EC2 ($HOST) is not reachable" >&2
  exit 1
fi

echo "==> Sync EdgeFabric to $HOST:/opt/edgefabric"
$SSH 'sudo mkdir -p /opt/edgefabric /var/lib/edgefabric /var/lib/edgefabric-cargo-target /etc/edgefabric && sudo chown -R ubuntu:ubuntu /opt/edgefabric /var/lib/edgefabric /var/lib/edgefabric-cargo-target'
linux_build_rsync_sources "$EDGEFABRIC_ROOT" "${USER}@$HOST:/opt/edgefabric"

VEEROS_ROOT="${VEEROS_ROOT:-$(cd "$EDGEFABRIC_ROOT/../VeerOS" && pwd)}"
if [[ -d "$VEEROS_ROOT/crates/quantum" ]]; then
  echo "==> Sync VeerOS to $HOST:/opt/VeerOS (EdgeFabric path dependency)"
  $SSH 'sudo mkdir -p /opt/VeerOS && sudo chown -R ubuntu:ubuntu /opt/VeerOS'
  rsync -az --delete \
    --exclude target --exclude .git --exclude demo_output.txt --exclude output.txt --exclude output.log \
    -e "$LINUX_BUILD_RSYNC" \
    "$VEEROS_ROOT/" "${USER}@$HOST:/opt/VeerOS/"
else
  echo "WARN: VeerOS not found at $VEEROS_ROOT — ef-api build will fail without ../../../VeerOS/crates/quantum" >&2
fi

DASHBOARD_DIST="${EDGEFABRIC_ROOT}/crates/ef-dashboard/dist"
if [[ -f "$DASHBOARD_DIST/index.html" ]]; then
  echo "==> Sync dashboard to $HOST:/var/www/edgefabric-dashboard"
  $SSH 'sudo mkdir -p /var/www/edgefabric-dashboard && sudo chown ubuntu:ubuntu /var/www/edgefabric-dashboard'
  rsync -az -e "$LINUX_BUILD_RSYNC" "$DASHBOARD_DIST/" "${USER}@$HOST:/var/www/edgefabric-dashboard/"
else
  echo "WARN: dashboard dist missing — run: cd crates/ef-dashboard && trunk build --release" >&2
fi

HOST_PKG_DIR="${EDGEFABRIC_ROOT}/dist/host"
if ls "$HOST_PKG_DIR"/edgefabric-host-*.tar.gz >/dev/null 2>&1 || ls "$HOST_PKG_DIR"/edgefabric-host-*.zip >/dev/null 2>&1; then
  echo "==> Sync host packages to $HOST downloads"
  $SSH 'sudo mkdir -p /var/www/edgefabric-dashboard/downloads /opt/edgefabric/dist/host && sudo chown -R ubuntu:ubuntu /var/www/edgefabric-dashboard/downloads /opt/edgefabric/dist/host'
  rsync -az -e "$LINUX_BUILD_RSYNC" --include 'edgefabric-host-*.tar.gz' --include 'edgefabric-host-*.zip' --exclude '*' \
    "$HOST_PKG_DIR/" "${USER}@$HOST:/var/www/edgefabric-dashboard/downloads/"
  rsync -az -e "$LINUX_BUILD_RSYNC" --include 'edgefabric-host-*.tar.gz' --include 'edgefabric-host-*.zip' --exclude '*' \
    "$HOST_PKG_DIR/" "${USER}@$HOST:/opt/edgefabric/dist/host/"
fi

if [[ "$SKIP_EC2_BUILD" -eq 1 ]]; then
  echo "==> Install LAN-built binaries on EC2"
  STAGE="$(mktemp -d)"
  trap 'rm -rf "$STAGE"' EXIT
  rsync -az -e "$LAN_RSYNC" \
    "${BUILD_HOST}:$(linux_build_lan_cargo_target)/release/ef-api" "$STAGE/" 2>/dev/null || true
  if [[ "$DEPLOY_AGENT" == "1" ]]; then
    rsync -az -e "$LAN_RSYNC" \
      "${BUILD_HOST}:$(linux_build_lan_cargo_target)/release/ef-agent" "$STAGE/" 2>/dev/null || true
  fi
  $SSH "mkdir -p ${EF_EC2_TARGET}/release"
  rsync -az -e "$LINUX_BUILD_RSYNC" "$STAGE/" "${USER}@$HOST:${EF_EC2_TARGET}/release/"
fi

echo "==> Remote install"
$SSH "SKIP_EC2_BUILD=$SKIP_EC2_BUILD EF_CARGO_TARGET=$EF_EC2_TARGET FORCE_BUILD=${FORCE_BUILD:-0} SKIP_INFRA=${SKIP_INFRA:-0} DEPLOY_AGENT=$DEPLOY_AGENT EF_PUBLIC_URL=$PUBLIC_URL bash -s" <<'REMOTE'
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

# Rust toolchain
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup default stable

sudo apt-get update -qq
sudo apt-get install -y -qq build-essential pkg-config libssl-dev clang libc6-dev protobuf-compiler

# Docker (reuse eGenie bootstrap if present)
if ! command -v docker >/dev/null; then
  echo "Docker not found — install eGenie bootstrap-host.sh first, or install Docker manually." >&2
  exit 1
fi

if [[ "${SKIP_INFRA:-0}" != "1" ]]; then
  echo "==> Start EdgeFabric infra (Postgres, Redis, NATS on 127.0.0.1)"
  cd /opt/edgefabric
  docker compose -f deploy/ec2/infra-compose.ec2.yml up -d
  for i in $(seq 1 30); do
    if docker compose -f deploy/ec2/infra-compose.ec2.yml exec -T postgres pg_isready -U edgefabric >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
fi

export EF_CARGO_TARGET="${EF_CARGO_TARGET:-/var/lib/edgefabric-cargo-target}"
mkdir -p "$EF_CARGO_TARGET/release"
export DATABASE_URL="${DATABASE_URL:-postgres://edgefabric:edgefabric@127.0.0.1:5435/edgefabric}"
export EF_RUN_MODE=production

if [[ "${SKIP_EC2_BUILD:-0}" != "1" && "${SKIP_BUILD:-0}" != "1" ]]; then
  if [[ "${FORCE_BUILD:-0}" == "1" || ! -x "$EF_CARGO_TARGET/release/ef-api" || "$EF_CARGO_TARGET/release/ef-api" -ot /opt/edgefabric/Cargo.toml ]]; then
    echo "Building ef-api on EC2 (cache: $EF_CARGO_TARGET)..."
    cd /opt/edgefabric
    CARGO_TARGET_DIR="$EF_CARGO_TARGET" cargo build --release -p ef-api
  else
    echo "Skipping ef-api compile (cached; FORCE_BUILD=1 to rebuild)"
  fi
  if [[ "${DEPLOY_AGENT:-0}" == "1" ]]; then
    if [[ "${FORCE_BUILD:-0}" == "1" || ! -x "$EF_CARGO_TARGET/release/ef-agent" ]]; then
      echo "Building ef-agent on EC2..."
      cd /opt/edgefabric
      CARGO_TARGET_DIR="$EF_CARGO_TARGET" cargo build --release -p ef-agent
    fi
  fi
else
  echo "Skipping EC2 compile (LAN-built or SKIP_BUILD)"
fi

sudo install -m 755 "$EF_CARGO_TARGET/release/ef-api" /usr/local/bin/ef-api
if [[ -x "$EF_CARGO_TARGET/release/ef-agent" ]]; then
  sudo install -m 755 "$EF_CARGO_TARGET/release/ef-agent" /usr/local/bin/ef-agent
fi

JWT_SECRET="${EF_JWT_SECRET:-}"
if [[ -z "$JWT_SECRET" && -f /etc/edgefabric/edgefabric.env ]]; then
  JWT_SECRET=$(sudo grep -E '^EF__AUTH__JWT_SECRET=' /etc/edgefabric/edgefabric.env 2>/dev/null | cut -d= -f2- || true)
fi
if [[ -z "$JWT_SECRET" ]]; then
  JWT_SECRET=$(openssl rand -hex 32)
fi

INTEGRATIONS_KEY="${EF_INTEGRATIONS_SECRET_KEY:-}"
if [[ -z "$INTEGRATIONS_KEY" ]]; then
  INTEGRATIONS_KEY=$(openssl rand -hex 32)
fi

PUBLIC_URL="${EF_PUBLIC_URL:-https://edgefabric.veerlabs.solutions}"
HOSTNAME_FQDN="$(echo "$PUBLIC_URL" | sed -E 's#https?://##')"

sudo tee /etc/edgefabric/edgefabric.env >/dev/null <<ENV
EF_RUN_MODE=production
EF__SERVER__HOST=127.0.0.1
EF__SERVER__PORT=6060
EF__SERVER__GRPC_PORT=50051
EF__DATABASE__URL=postgres://edgefabric:edgefabric@127.0.0.1:5435/edgefabric
EF__REDIS__URL=redis://127.0.0.1:6379
EF__NATS__URL=nats://127.0.0.1:4222
EF__AUTH__JWT_SECRET=${JWT_SECRET}
EF_INTEGRATIONS_SECRET_KEY=${INTEGRATIONS_KEY}
EF__STREAMING__TURN_SERVER=turn:127.0.0.1:3478
EF__STREAMING__TURN_USERNAME=edgefabric
EF__STREAMING__TURN_PASSWORD=edgefabric
RUST_LOG=info,ef_api=info
ENV
sudo chmod 600 /etc/edgefabric/edgefabric.env

sudo tee /etc/systemd/system/ef-api.service >/dev/null <<'UNIT'
[Unit]
Description=EdgeFabric API
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
EnvironmentFile=/etc/edgefabric/edgefabric.env
WorkingDirectory=/opt/edgefabric
ExecStart=/usr/local/bin/ef-api
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT

if [[ "${DEPLOY_AGENT:-0}" == "1" && -x /usr/local/bin/ef-agent ]]; then
  sudo tee /etc/edgefabric/ef-agent.env >/dev/null <<AGENTENV
EF_RUN_MODE=production
EF__AGENT__CONTROL_PLANE_URL=http://127.0.0.1:6060
EF__AGENT__AGENT_ID=ec2-agent-01
EF__AGENT__REGION=us-east-1
AGENTENV
  sudo chmod 600 /etc/edgefabric/ef-agent.env

  sudo tee /etc/systemd/system/ef-agent.service >/dev/null <<'UNIT'
[Unit]
Description=EdgeFabric Agent
After=network-online.target ef-api.service
Wants=ef-api.service

[Service]
Type=simple
User=ubuntu
Group=ubuntu
EnvironmentFile=/etc/edgefabric/ef-agent.env
WorkingDirectory=/opt/edgefabric
ExecStart=/usr/local/bin/ef-agent
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT
fi

# Caddy — dashboard static files + API reverse proxy
CADDY_BLOCK="edgefabric.veerlabs.solutions"
if command -v caddy >/dev/null; then
  if ! sudo grep -q "$CADDY_BLOCK" /etc/caddy/Caddyfile 2>/dev/null; then
    echo "==> Adding Caddy block for $CADDY_BLOCK"
    sudo tee -a /etc/caddy/Caddyfile >/dev/null <<CADDY

${HOSTNAME_FQDN} {
	handle /api/* {
		reverse_proxy 127.0.0.1:6060
	}
	handle /health {
		reverse_proxy 127.0.0.1:6060
	}
	handle /metrics {
		reverse_proxy 127.0.0.1:6060
	}
	handle /swagger-ui* {
		reverse_proxy 127.0.0.1:6060
	}
	handle /api-docs* {
		reverse_proxy 127.0.0.1:6060
	}
	handle {
		root * /var/www/edgefabric-dashboard
		file_server
		try_files {path} /index.html
	}
}
CADDY
  fi
  sudo systemctl reload caddy || sudo systemctl restart caddy
fi

# Idle shutdown — add EdgeFabric port if eGenie idle config exists
if [[ -f /etc/egenie/idle-shutdown.conf ]]; then
  if sudo grep -q '^MONITOR_PORTS=' /etc/egenie/idle-shutdown.conf; then
    CURRENT=$(sudo grep '^MONITOR_PORTS=' /etc/egenie/idle-shutdown.conf | head -1)
    if ! echo "$CURRENT" | grep -q '6060'; then
      sudo sed -i 's/^MONITOR_PORTS="\([^"]*\)"/MONITOR_PORTS="\1 6060"/' /etc/egenie/idle-shutdown.conf
    fi
  fi
fi

sudo systemctl daemon-reload
sudo systemctl enable ef-api
sudo systemctl restart ef-api
if [[ "${DEPLOY_AGENT:-0}" == "1" && -f /etc/systemd/system/ef-agent.service ]]; then
  sudo systemctl enable ef-agent
  sudo systemctl restart ef-agent
fi

sleep 3
curl -sf http://127.0.0.1:6060/health && echo
sudo systemctl --no-pager status ef-api | head -15
REMOTE

echo ""
echo "Deploy complete."
echo "  Host:     $HOST"
echo "  API:      $PUBLIC_URL"
echo "  Health:   curl $PUBLIC_URL/health"
echo "  Swagger:  $PUBLIC_URL/swagger-ui"
echo ""
echo "DNS: point edgefabric.veerlabs.solutions → $HOST (same elastic IP as eGenie)"
echo "Security group: TCP 80, 443 (and UDP 10000-60000 if using WebRTC streaming)"
