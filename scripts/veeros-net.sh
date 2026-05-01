#!/usr/bin/env bash
set -euo pipefail

# Configure host TAP + DHCP for folded VeerOS VMs so veer-connect can reach
# the guest shell listener on TCP/2323.

TAP_IFACE="${EF_VEEROS_TAP_IFACE:-tap0}"
HOST_CIDR="${EF_VEEROS_HOST_CIDR:-10.77.0.1/24}"
DHCP_RANGE_START="${EF_VEEROS_DHCP_START:-10.77.0.10}"
DHCP_RANGE_END="${EF_VEEROS_DHCP_END:-10.77.0.200}"
DHCP_LEASE_TIME="${EF_VEEROS_DHCP_LEASE:-12h}"
DNS_SERVER="${EF_VEEROS_DNS:-1.1.1.1}"

PID_FILE="/tmp/edgefabric-dnsmasq-${TAP_IFACE}.pid"
LEASE_FILE="/tmp/edgefabric-dnsmasq-${TAP_IFACE}.leases"
CONF_FILE="/tmp/edgefabric-dnsmasq-${TAP_IFACE}.conf"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { err "Required command not found: $1"; exit 1; }
}

run_root() {
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    "$@"
    return
  fi
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return
  fi
  err "Root privileges are required for: $*"
  exit 1
}

dnsmasq_pid() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      echo "$pid"
      return 0
    fi
  fi

  # Fallback for hosts where dnsmasq starts but does not materialize pid-file.
  local proc_pid
  proc_pid="$(pgrep -f "dnsmasq .*--conf-file=${CONF_FILE}" | head -n 1 || true)"
  if [[ -n "$proc_pid" ]] && kill -0 "$proc_pid" 2>/dev/null; then
    echo "$proc_pid"
    return 0
  fi

  return 1
}

dnsmasq_is_listening() {
  ss -H -ulpn 2>/dev/null | grep -Eq "0\.0\.0\.0%${TAP_IFACE}:67[[:space:]]"
}

write_dnsmasq_conf() {
  local host_ip="${HOST_CIDR%/*}"
  cat > "$CONF_FILE" <<EOF
port=0
interface=${TAP_IFACE}
bind-interfaces
dhcp-authoritative
dhcp-range=${DHCP_RANGE_START},${DHCP_RANGE_END},255.255.255.0,${DHCP_LEASE_TIME}
dhcp-option=3,${host_ip}
dhcp-option=6,${DNS_SERVER}
dhcp-leasefile=${LEASE_FILE}
pid-file=${PID_FILE}
except-interface=lo
log-dhcp
EOF
}

ensure_tap() {
  local tap_owner="${SUDO_USER:-$USER}"
  if ! ip link show "$TAP_IFACE" >/dev/null 2>&1; then
    info "Creating TAP interface: $TAP_IFACE (owner: $tap_owner)"
    run_root ip tuntap add dev "$TAP_IFACE" mode tap user "$tap_owner"
  fi

  # Ensure host side has the fixed gateway IP.
  run_root ip addr add "$HOST_CIDR" dev "$TAP_IFACE" 2>/dev/null || true
  run_root ip link set "$TAP_IFACE" up
}

start_dnsmasq() {
  write_dnsmasq_conf

  local existing_pid
  existing_pid="$(dnsmasq_pid || true)"
  if [[ -n "$existing_pid" ]]; then
    info "Stopping previous dnsmasq (PID ${existing_pid})"
    run_root kill "$existing_pid" || true
    sleep 1
  fi

  # Clean stale pid file so dnsmasq can start reliably.
  rm -f "$PID_FILE"

  info "Starting dnsmasq on $TAP_IFACE (${DHCP_RANGE_START}-${DHCP_RANGE_END})"
  run_root dnsmasq --conf-file="$CONF_FILE"

  local started_pid
  started_pid="$(dnsmasq_pid || true)"
  if [[ -n "$started_pid" ]] || dnsmasq_is_listening; then
    ok "dnsmasq started (PID ${started_pid})"
  else
    err "dnsmasq did not start; check journal/syslog"
    exit 1
  fi
}

status() {
  local host_ip="${HOST_CIDR%/*}"
  echo "tap_iface=${TAP_IFACE}"
  ip -4 addr show "$TAP_IFACE" 2>/dev/null | sed 's/^/  /' || warn "tap interface not found"
  echo "expected_host_ip=${host_ip}"
  local pid
  pid="$(dnsmasq_pid || true)"
  if [[ -n "$pid" ]] || dnsmasq_is_listening; then
    ok "dnsmasq running (PID ${pid})"
  else
    warn "dnsmasq not running"
  fi
  if [[ -f "$LEASE_FILE" ]]; then
    echo "leases_file=$LEASE_FILE"
    tail -n 20 "$LEASE_FILE" | sed 's/^/  /' || true
  fi
}

stop() {
  local pid
  pid="$(dnsmasq_pid || true)"
  if [[ -n "$pid" ]]; then
    info "Stopping dnsmasq (PID ${pid})"
    run_root kill "$pid" || true
  else
    warn "dnsmasq is not running"
  fi
  rm -f "$PID_FILE"
}

usage() {
  cat <<EOF
Usage: $0 <up|down|status>

Commands:
  up      Ensure ${TAP_IFACE} is up with ${HOST_CIDR}, start dnsmasq DHCP
  down    Stop dnsmasq started by this script
  status  Show TAP and DHCP status

Environment overrides:
  EF_VEEROS_TAP_IFACE   (default: tap0)
  EF_VEEROS_HOST_CIDR   (default: 10.77.0.1/24)
  EF_VEEROS_DHCP_START  (default: 10.77.0.10)
  EF_VEEROS_DHCP_END    (default: 10.77.0.200)
  EF_VEEROS_DHCP_LEASE  (default: 12h)
  EF_VEEROS_DNS         (default: 1.1.1.1)
EOF
}

need_cmd ip
need_cmd dnsmasq

case "${1:-status}" in
  up)
    ensure_tap
    start_dnsmasq
    status
    ;;
  down)
    stop
    ;;
  status)
    status
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    err "Unknown command: ${1:-}"
    usage
    exit 1
    ;;
esac
