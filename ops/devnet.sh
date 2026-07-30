#!/usr/bin/env bash
#
# A local Hikmalayer chain for development, in one command.
#
#   ops/devnet.sh
#
# Starts a single-node chain with a funded treasury, a registered validator,
# and a miner that seals a block every few seconds so queued transactions
# actually execute. Ctrl-C stops everything and leaves no state behind unless
# you asked it to.
#
# This is a DEVELOPMENT network. It prints its keys, uses fixed tokens, and
# enables the faucet. Nothing here belongs on a network you care about — see
# ops/start_testnet.sh for a multi-node testnet, and docs/mainnet_readiness.md
# for a real launch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PORT="${PORT:-3000}"
DATA_DIR="${DATA_DIR:-$ROOT/.devnet}"
ADMIN_TOKEN="${ADMIN_TOKEN:-devadmin}"
P2P_TOKEN="${P2P_TOKEN:-devp2p}"
BLOCK_SECONDS="${BLOCK_SECONDS:-5}"
KEEP_STATE="${KEEP_STATE:-0}"
# Origins the dashboard may be served from during development.
CORS="${CORS_ALLOWED_ORIGINS:-http://localhost:5173,http://127.0.0.1:5173,http://localhost:4173,http://127.0.0.1:4173}"

say()  { printf '\033[36m%s\033[0m\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*"; }

# ---------------------------------------------------------------- build

if [[ ! -x target/release/hikmalayer || ! -x target/release/hikma-wallet ]]; then
  say "Building the node and wallet (first run only)…"
  cargo build --release --bin hikmalayer --bin hikma-wallet
fi

# ---------------------------------------------------------------- keys
#
# A fixed key keeps the devnet reproducible: restart it and your test accounts
# and addresses are unchanged. Override with DEVNET_KEY to get a fresh chain.
DEVNET_KEY="${DEVNET_KEY:-80f91adc283392febbfc86b7327c055b8559373459040e07e78640e3ac592517}"

keyinfo() { ./target/release/hikma-wallet address "$1"; }
derived=$(./target/release/hikma-wallet sign-transfer a b 1 0 "$DEVNET_KEY")
PUBLIC_KEY=$(awk '/public_key:/{print $2}' <<<"$derived")
ADDRESS=$(./target/release/hikma-wallet address "$PUBLIC_KEY" | awk '/address:/{print $2}')
VRF_KEY=$(./target/release/hikma-wallet sign-stake "$ADDRESS" 1 0 "$DEVNET_KEY" \
          | awk '/vrf_public_key:/{print $2}')

# ---------------------------------------------------------------- state

if [[ "$KEEP_STATE" != "1" ]]; then
  rm -rf "$DATA_DIR"
fi
mkdir -p "$DATA_DIR"

cleanup() {
  local code=$?
  [[ -n "${MINER_PID:-}" ]] && kill "$MINER_PID" 2>/dev/null || true
  [[ -n "${NODE_PID:-}"  ]] && kill "$NODE_PID"  2>/dev/null || true
  exit "$code"
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------- node

say "Starting the node on http://127.0.0.1:$PORT …"
VALIDATOR_PRIVATE_KEY="$DEVNET_KEY" \
TREASURY_PRIVATE_KEY="$DEVNET_KEY" \
GENESIS_TREASURY_ADDRESS="$ADDRESS" \
GENESIS_VALIDATOR_PUBLIC_KEY="$PUBLIC_KEY" \
GENESIS_VALIDATOR_VRF_PUBLIC_KEY="$VRF_KEY" \
ADMIN_TOKEN="$ADMIN_TOKEN" \
P2P_TOKEN="$P2P_TOKEN" \
CORS_ALLOWED_ORIGINS="$CORS" \
HIKMALAYER_STATE_PATH="$DATA_DIR/state.json" \
PORT="$PORT" \
./target/release/hikmalayer > "$DATA_DIR/node.log" 2>&1 &
NODE_PID=$!

# Wait for it to answer rather than guessing with a sleep.
for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:$PORT/blockchain/stats" >/dev/null 2>&1; then break; fi
  if ! kill -0 "$NODE_PID" 2>/dev/null; then
    warn "The node exited during startup. Last lines of $DATA_DIR/node.log:"
    tail -20 "$DATA_DIR/node.log"
    exit 1
  fi
  sleep 0.5
done

if ! curl -fsS "http://127.0.0.1:$PORT/blockchain/stats" >/dev/null 2>&1; then
  warn "The node did not become ready in time. See $DATA_DIR/node.log"
  exit 1
fi

# ---------------------------------------------------------------- miner
#
# Transactions are only queued on submission; without a miner nothing takes
# effect and every tutorial appears broken. This seals a block on a timer.
(
  while sleep "$BLOCK_SECONDS"; do
    curl -fsS -X POST "http://127.0.0.1:$PORT/mine" \
      -H "x-admin-token: $ADMIN_TOKEN" >/dev/null 2>&1 || true
  done
) &
MINER_PID=$!

VALIDATORS=$(curl -fsS "http://127.0.0.1:$PORT/staking/validators" | grep -c 'address' || true)
BALANCE=$(curl -fsS "http://127.0.0.1:$PORT/tokens/balance/$ADDRESS" \
          | sed 's/.*"balance":\([0-9]*\).*/\1/')

cat <<INFO

$(say "Hikmalayer devnet is up.")

  RPC            http://127.0.0.1:$PORT
  Admin token    $ADMIN_TOKEN
  Treasury       $ADDRESS
  Balance        $BALANCE base units  (1 HKM = 1,000,000)
  Validator      registered, sealing a block every ${BLOCK_SECONDS}s
  State          $DATA_DIR/state.json
  Log            $DATA_DIR/node.log

Try it:

  curl -s http://127.0.0.1:$PORT/blockchain/stats

  # Fund an account (devnet faucet)
  curl -s -X POST http://127.0.0.1:$PORT/tokens/faucet \\
    -H 'content-type: application/json' -H 'x-admin-token: $ADMIN_TOKEN' \\
    -d '{"to":"hkm…","amount":"100000000"}'

  # Or use the SDK
  cd sdk && npm install
  HIKMALAYER_ADMIN_TOKEN=$ADMIN_TOKEN npm run test:integration

$(warn "Development only: fixed keys, printed secrets, faucet enabled.")
Press Ctrl-C to stop.

INFO

wait "$NODE_PID"
