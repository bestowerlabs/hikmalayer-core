#!/bin/bash
# Hikmalayer block production loop.
# Calls POST /mine on a fixed interval so the bootnode actually advances.
# Same mechanism as ops/devnet.sh, wrapped for systemd instead of a shell subshell.

set -u

ADMIN_TOKEN="$(grep '^ADMIN_TOKEN=' /home/hikmalayer/hikmalayer-core/.env | cut -d= -f2-)"
BLOCK_SECONDS=5
PORT=3000

if [ -z "$ADMIN_TOKEN" ]; then
  echo "ADMIN_TOKEN not found in .env — refusing to start" >&2
  exit 1
fi

while sleep "$BLOCK_SECONDS"; do
  curl -fsS -X POST "http://127.0.0.1:${PORT}/mine" \
    -H "x-admin-token: ${ADMIN_TOKEN}" >/dev/null 2>&1 || \
    echo "$(date -Is) mine attempt failed" >&2
done
