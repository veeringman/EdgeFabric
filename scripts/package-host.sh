#!/usr/bin/env bash
# Build host-join binaries for the current OS/arch into dist/host/<triple>/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
OUT="$ROOT/dist/host/${TRIPLE}"
mkdir -p "$OUT"

echo "building ef, ef-agent, ef-ml-worker ($TRIPLE)"
cargo build --release -p ef-cli -p ef-agent -p ef-ml-worker

BIN_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/release"
install -m 0755 "$BIN_DIR/ef" "$OUT/ef"
install -m 0755 "$BIN_DIR/ef-agent" "$OUT/ef-agent"
install -m 0755 "$BIN_DIR/ef-ml-worker" "$OUT/ef-ml-worker"
if [[ -f "$ROOT/crates/ef-ml-worker/runtime/mlx_runtime.py" ]]; then
  mkdir -p "$OUT/runtime"
  cp "$ROOT/crates/ef-ml-worker/runtime/mlx_runtime.py" "$OUT/runtime/"
fi

if [[ -x "${VEERSETU_AGENT:-}" ]]; then
  install -m 0755 "$VEERSETU_AGENT" "$OUT/veersetu-agent"
elif [[ -x "$HOME/.veersetu/bin/veersetu-agent" ]]; then
  install -m 0755 "$HOME/.veersetu/bin/veersetu-agent" "$OUT/veersetu-agent"
elif [[ -x "$ROOT/../VeerSetu/target/release/veersetu-agent" ]]; then
  install -m 0755 "$ROOT/../VeerSetu/target/release/veersetu-agent" "$OUT/veersetu-agent"
elif [[ -x /opt/veersetu-src/target/release/veersetu-agent ]]; then
  install -m 0755 /opt/veersetu-src/target/release/veersetu-agent "$OUT/veersetu-agent"
elif [[ -x /var/lib/veersetu-cargo-target/release/veersetu-agent ]]; then
  install -m 0755 /var/lib/veersetu-cargo-target/release/veersetu-agent "$OUT/veersetu-agent"
fi

cp "$ROOT/packaging/host/install.sh" "$OUT/install.sh"
chmod +x "$OUT/install.sh"
cp "$ROOT/packaging/host/README.md" "$OUT/README.md"

ARCHIVE="$ROOT/dist/host/edgefabric-host-${TRIPLE}.tar.gz"
tar -C "$OUT" -czf "$ARCHIVE" .
mkdir -p "$ROOT/crates/ef-dashboard/dist/downloads"
cp "$ARCHIVE" "$ROOT/crates/ef-dashboard/dist/downloads/"
echo "package: $ARCHIVE"
ls -lh "$OUT"
