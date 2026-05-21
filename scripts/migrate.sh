#!/usr/bin/env bash
# EdgeFabric Database Migration Runner
# Usage: ./scripts/migrate.sh [up|status|revert]
#
# Environment:
#   DATABASE_URL  — PostgreSQL connection string (required)
#
set -euo pipefail

COMMAND="${1:-up}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
DEFAULT_DATABASE_URL="postgres://edgefabric:edgefabric@localhost:5432/edgefabric"

DATABASE_URL="${DATABASE_URL:-$DEFAULT_DATABASE_URL}"

# Ensure sqlx-cli is available
if ! command -v sqlx &>/dev/null; then
  echo "Installing sqlx-cli..." >&2
  cargo install sqlx-cli --no-default-features --features postgres
fi

case "$COMMAND" in
  up)
    echo "Running migrations from $MIGRATIONS_DIR ..."
    sqlx migrate run --source "$MIGRATIONS_DIR" --database-url "$DATABASE_URL"
    echo "Migrations complete."
    ;;
  status)
    sqlx migrate info --source "$MIGRATIONS_DIR" --database-url "$DATABASE_URL"
    ;;
  revert)
    echo "Reverting last migration..."
    sqlx migrate revert --source "$MIGRATIONS_DIR" --database-url "$DATABASE_URL"
    echo "Revert complete."
    ;;
  *)
    echo "Usage: $0 [up|status|revert]" >&2
    exit 1
    ;;
esac
