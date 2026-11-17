#!/usr/bin/env bash
# GitHub Actions performs the macOS/Windows builds; this command prepares and tracks them.
set -euo pipefail
GRID_DEPLOY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$GRID_DEPLOY_ROOT/packaging/deploy.py" "$@"
