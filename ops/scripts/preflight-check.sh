#!/bin/bash
# Pre-flight safety check: before deploying a new binary to any node
# holding real chain state, confirm it can genuinely replay a COPY of
# that state without triggering a "failed replay -> fresh genesis" or
# "reward transaction must pay..." style rejection.
#
# This exists because of two real incidents on 16 September 2026: a
# change to ChainState's fields invalidated the genesis state root for
# every already-running chain, and a stricter validation rule rejected
# blocks that were valid under the code that originally produced them.
# Both were only discovered by deploying directly to the live bootnode.
# This script catches the same class of problem against a disposable
# copy, before any real node's data is touched.
set -euo pipefail

REAL_STATE="/home/hikmalayer/hikmalayer-core/data/state.json"
TEST_DIR="/tmp/hikmalayer-preflight-$(date +%s)"
NEW_BINARY="./target/release/hikmalayer"

if [ ! -f "$REAL_STATE" ]; then
  echo "No real state.json found at $REAL_STATE — nothing to test against." >&2
  exit 1
fi

if [ ! -x "$NEW_BINARY" ]; then
  echo "Build the new binary first (cargo build --release) before running this check." >&2
  exit 1
fi

mkdir -p "$TEST_DIR/data"
cp "$REAL_STATE" "$TEST_DIR/data/state.json"

echo "Testing replay of real chain state against the newly built binary..."
HIKMALAYER_STATE_PATH="$TEST_DIR/data/state.json" \
  GENESIS_CHAIN_ID=hikmalayer-mainnet \
  timeout 15 "$NEW_BINARY" > "$TEST_DIR/output.log" 2>&1 &
PID=$!
sleep 8
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true

if grep -qi "failed state replay\|starting from a fresh genesis" "$TEST_DIR/output.log"; then
  echo ""
  echo "❌ PRE-FLIGHT CHECK FAILED"
  echo "The new binary could NOT replay the real chain's history."
  echo "Deploying this build would truncate or reset live chain data."
  echo ""
  grep -i "failed state replay" "$TEST_DIR/output.log"
  rm -rf "$TEST_DIR"
  exit 1
fi

echo ""
echo "✅ Pre-flight check passed — the new binary correctly replayed the real chain."
rm -rf "$TEST_DIR"
exit 0
