#!/usr/bin/env bash
# Install EdgeFabric host worker binaries onto this machine (macOS / Linux).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${EF_PREFIX:-$HOME/.edgefabric}"
BIN="$PREFIX/bin"
mkdir -p "$BIN" "$PREFIX/logs"

copy_bin() {
  local name="$1"
  if [[ -x "$ROOT/$name" ]]; then
    install -m 0755 "$ROOT/$name" "$BIN/$name"
    echo "installed $BIN/$name"
  else
    echo "skip $name (not in this package)" >&2
  fi
}

copy_bin ef
copy_bin ef-agent
copy_bin ef-ml-worker
copy_bin veersetu-agent
if [[ -d "$ROOT/runtime" ]]; then
  mkdir -p "$PREFIX/runtime"
  cp -R "$ROOT/runtime/"* "$PREFIX/runtime/" 2>/dev/null || true
fi

# PATH hint
SHELL_RC="${SHELL_RC:-}"
if [[ -z "$SHELL_RC" ]]; then
  if [[ -n "${ZSH_VERSION:-}" ]] || [[ "${SHELL:-}" == *zsh* ]]; then
    SHELL_RC="$HOME/.zshrc"
  else
    SHELL_RC="$HOME/.bashrc"
  fi
fi
if ! grep -q '.edgefabric/bin' "$SHELL_RC" 2>/dev/null; then
  echo "export PATH=\"\$HOME/.edgefabric/bin:\$PATH\"" >> "$SHELL_RC"
  echo "appended PATH to $SHELL_RC"
fi
export PATH="$BIN:$PATH"

SERVER="${EF_SERVER_URL:-https://edgefabric.veerlabs.solutions}"
if [[ -n "${EF_API_KEY:-}" ]]; then
  JOIN_ARGS=(--server "$SERVER" host join --api-key "$EF_API_KEY")
  [[ -n "${EF_HOST_NAME:-}" ]] && JOIN_ARGS+=(--name "$EF_HOST_NAME")
  [[ "${EF_WITH_ML:-0}" == "1" ]] && JOIN_ARGS+=(--with-ml)
  "$BIN/ef" "${JOIN_ARGS[@]}"
else
  echo "Binaries installed. Join with:"
  echo "  export PATH=\"$BIN:\$PATH\""
  echo "  ef --server $SERVER host join --api-key <tenant-key> [--with-ml]"
fi

if [[ -x "$BIN/veersetu-agent" && -n "${VEERSETU_CONTROL_TOKEN:-}" ]]; then
  NODE="$(hostname | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9' '-' | sed 's/-*$//')"
  RELAY="${VEERSETU_RELAY:-100.52.147.238:51820}"
  CONTROL="${VEERSETU_CONTROL_URL:-http://veersetu-rd.veerlabs.solutions:51821}"
  SNI="${VEERSETU_SERVER_NAME:-veersetu-rd.veerlabs.solutions}"
  echo "starting veersetu-agent service=ssh-$NODE"
  nohup env VEERSETU_CONTROL_TOKEN="$VEERSETU_CONTROL_TOKEN" \
    "$BIN/veersetu-agent" \
    --relay "$RELAY" \
    --server-name "$SNI" \
    --control-url "$CONTROL" \
    --service "ssh-$NODE" \
    --upstream 127.0.0.1:22 \
    --node-id "$NODE" \
    >"$PREFIX/logs/veersetu-agent.log" 2>&1 &
  echo $! > "$PREFIX/veersetu-agent.pid"
fi

echo "done. Next: enable Remote Login / sshd, then check dashboard Devices & Hosts."
