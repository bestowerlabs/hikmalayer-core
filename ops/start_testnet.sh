#!/usr/bin/env bash
set -euo pipefail
# Boots the local Docker testnet, wires the FULL P2P mesh, and registers
# validators 2..5 ON-CHAIN. Keys and tokens are loaded from .env — never
# hardcoded here (see .env.example).
#
# Block production drives /mine across ALL validator nodes: PoS leadership
# is stake-weighted per height (with timeout-based fallback rotation), so
# whichever node is the eligible leader produces the block. Driving only
# the bootnode would stall every height led by another validator.

if [[ -f ".env" ]]; then
  set -a
  source .env
  set +a
else
  echo "ERROR: .env file not found. Copy .env.example to .env and fill in real values."
  exit 1
fi

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
BOOT="http://localhost:3000"
# Host-mapped node URLs (leader-probe order) and their in-network names.
NODES=(
  "http://localhost:3000"
  "http://localhost:3001"
  "http://localhost:3002"
  "http://localhost:3003"
  "http://localhost:3004"
  "http://localhost:3010"
)
PEER_NAMES=(
  "http://bootnode:3000"
  "http://validator1:3000"
  "http://validator2:3000"
  "http://validator3:3000"
  "http://validator4:3000"
  "http://rpc:3000"
)

WALLET="./target/release/hikma-wallet"
if [[ ! -x "${WALLET}" ]]; then
  echo "Building hikma-wallet..."
  cargo build --release --bin hikma-wallet
fi

echo "Starting Hikmalayer testnet..."
docker compose -f "${COMPOSE_FILE}" up -d --build

wait_up() {
  for _ in $(seq 1 60); do
    if curl -sf "$1/blockchain/stats" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  echo "ERROR: $1 did not come up"
  return 1
}

echo "Waiting for all nodes to boot..."
for node in "${NODES[@]}"; do wait_up "${node}"; done

# Wire the full P2P mesh FIRST, on EVERY node, so transactions gossip to all
# validators before any of them needs to mine, and mined blocks gossip back.
# (Registering peers only on the bootnode — and only at the end — left the
# other validators isolated: they had no pending transactions to mine when
# PoS selected them, stalling block production.)
echo "Registering the full peer mesh on every node..."
for i in "${!NODES[@]}"; do
  node="${NODES[$i]}"
  self="${PEER_NAMES[$i]}"
  for peer in "${PEER_NAMES[@]}"; do
    [[ "${peer}" == "${self}" ]] && continue
    curl -s -X POST "${node}/p2p/peers/register" \
      -H "Content-Type: application/json" \
      -H "x-p2p-token: ${P2P_TOKEN}" \
      -d "{\"address\":\"${peer}\"}" >/dev/null || true
  done
done

# Drive /mine across every validator node: the PoS-selected leader (or a
# timeout fallback) produces the block, the rest answer "not eligible".
mine_any() {
  for _ in $(seq 1 30); do
    for node in "${NODES[@]}"; do
      resp=$(curl -s -X POST "${node}/mine" || true)
      if [[ "${resp}" == *'"status":"success"'* ]]; then
        sleep 1 # let the block gossip to the rest of the mesh
        return 0
      fi
    done
    sleep 1
  done
  echo "WARNING: no node produced a block (is a leader offline?)"
  return 1
}

pubkey_of() { "${WALLET}" sign-stake x 1 1 "$1" | awk '$1=="public_key:"{print $2}'; }
address_of() { "${WALLET}" address "$1" | awk '{print $2}'; }

echo "Registering validators 2..5 on-chain..."
VALIDATOR_KEYS=("${VALIDATOR1_KEY}" "${VALIDATOR2_KEY}" "${VALIDATOR3_KEY}" "${VALIDATOR4_KEY}")

# Amounts are in base units (6 decimals: 1 HKM = 1,000,000 units).
# Each validator stakes the on-chain minimum (10,000 HKM) and is funded
# with a margin for transaction fees.
STAKE_UNITS=10000000000      # 10,000 HKM = MIN_VALIDATOR_STAKE
FUND_UNITS=10100000000       # 10,100 HKM (stake + fee margin)

for sk in "${VALIDATOR_KEYS[@]}"; do
  pub=$(pubkey_of "$sk")
  addr=$(address_of "$pub")

  # Fund from the faucet (treasury transfer) and mine it in.
  curl -s -X POST "${BOOT}/tokens/faucet" \
    -H "Content-Type: application/json" \
    -H "x-admin-token: ${ADMIN_TOKEN}" \
    -d "{\"to\":\"${addr}\",\"amount\":${FUND_UNITS}}" >/dev/null
  mine_any

  # Sign and submit the on-chain stake (binds the VRF key), then mine it in.
  nonce=$(curl -s "${BOOT}/tokens/nonce/${addr}" | sed 's/.*"next_nonce":\([0-9]*\).*/\1/')
  stake_out=$("${WALLET}" sign-stake "${addr}" "${STAKE_UNITS}" "${nonce}" "${sk}")
  sig=$(echo "${stake_out}" | awk '/signature:/{print $2}')
  vrf_pub=$(echo "${stake_out}" | awk '/vrf_public_key:/{print $2}')

  curl -s -X POST "${BOOT}/staking/deposit" \
    -H "Content-Type: application/json" \
    -d "{\"address\":\"${addr}\",\"amount\":${STAKE_UNITS},\"public_key\":\"${pub}\",\"vrf_public_key\":\"${vrf_pub}\",\"nonce\":${nonce},\"signature\":\"${sig}\"}" >/dev/null
  mine_any
  echo "  registered ${addr} (stake ${STAKE_UNITS} units = 10,000 HKM)"
done

echo "Testnet started. Validator set:"
curl -s "${BOOT}/staking/validators"
echo
echo "State: $(curl -s "${BOOT}/blockchain/state")"
