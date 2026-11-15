#!/usr/bin/env bash
# Verify the app as delivered by the disk image, not just the staging directory.
set -euo pipefail
IMAGE="${1:?usage: check-dmg.sh <image.dmg>}"
MOUNT="$(mktemp -d "${TMPDIR:-/tmp}/grid-dmg-check.XXXXXX")"
ATTACHED=0
cleanup() {
  if [[ "$ATTACHED" = 1 ]]; then hdiutil detach "$MOUNT" >/dev/null || true; fi
  rmdir "$MOUNT" 2>/dev/null || true
}
trap cleanup EXIT
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$IMAGE"
ATTACHED=1
codesign --verify --strict --deep --verbose=2 "$MOUNT/Grid.app"
