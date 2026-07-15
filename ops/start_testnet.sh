#!/usr/bin/env bash
set -euo pipefail
# Boots the local Docker testnet and registers validators 2..5 ON-CHAIN.
# Keys and tokens are loaded from .env — never hardcoded here.

# Load .env file
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
WALLET="./target/release/hikma-wallet"

if [[ ! -x "${WALLET}" ]]; then
  echo "Building hikma-wallet..."
  cargo build --release --bin hikma-wallet
fi

echo "Starting Hikmalayer testnet..."
docker compose -f "${COMPOSE_FILE}" up -d --build

echo "Waiting for the bootnode to boot..."
for _ in $(seq 1 30); do
  if curl -sf "${BOOT}/blockchain/stats" >/dev/null 2>&1; then break; fi
  sleep 1
done

pubkey_of() { "${WALLET}" sign-stake x 1 1 "$1" | awk '$1=="public_key:"{print $2}'; }
address_of() { "${WALLET}" address "$1" | awk '{print $2}'; }

echo "Registering validators 2..5 on-chain via the bootnode..."

# Use real keys from .env instead of dev keys
VALIDATOR_KEYS=("${VALIDATOR1_KEY}" "${VALIDATOR2_KEY}" "${VALIDATOR3_KEY}" "${VALIDATOR4_KEY}")

for sk in "${VALIDATOR_KEYS[@]}"; do
  pub=$(pubkey_of "$sk")
  addr=$(address_of "$pub")

  # Fund from the faucet (treasury transfer) and mine it in.
  curl -s -X POST "${BOOT}/tokens/faucet" \
    -H "Content-Type: application/json" \
    -H "x-admin-token: ${ADMIN_TOKEN}" \
    -d "{\"to\":\"${addr}\",\"amount\":300}" >/dev/null
  curl -s -X POST "${BOOT}/mine" >/dev/null

  # Sign and submit the on-chain stake (binds the VRF key), then mine it in.
  nonce=$(curl -s "${BOOT}/tokens/nonce/${addr}" | sed 's/.*"next_nonce":\([0-9]*\).*/\1/')
  stake_out=$("${WALLET}" sign-stake "${addr}" 100 "${nonce}" "${sk}")
  sig=$(echo "${stake_out}" | awk '/signature:/{print $2}')
  vrf_pub=$(echo "${stake_out}" | awk '/vrf_public_key:/{print $2}')

  curl -s -X POST "${BOOT}/staking/deposit" \
    -H "Content-Type: application/json" \
    -d "{\"address\":\"${addr}\",\"amount\":100,\"public_key\":\"${pub}\",\"vrf_public_key\":\"${vrf_pub}\",\"nonce\":${nonce},\"signature\":\"${sig}\"}" >/dev/null
  curl -s -X POST "${BOOT}/mine" >/dev/null
  echo "  registered ${addr} (stake 100)"
done

echo "Registering peer mesh (bootnode → validators)..."
for peer in http://validator1:3000 http://validator2:3000 http://validator3:3000 http://validator4:3000 http://rpc:3000; do
  curl -s -X POST "${BOOT}/p2p/peers/register" \
    -H "Content-Type: application/json" \
    -H "x-p2p-token: ${P2P_TOKEN}" \
    -d "{\"address\":\"${peer}\"}" >/dev/null || true
done

echo "Testnet started. Validator set:"
curl -s "${BOOT}/staking/validators"
echo
echo "State: $(curl -s "${BOOT}/blockchain/state")"#!/usr/bin/env bash
set -euo pipefail

# Boots the local Docker testnet and registers validators 2..5 ON-CHAIN.
#
# The bootnode holds the dev genesis validator/treasury key (0x…01), so it can
# produce blocks and fund accounts from genesis. The other validators are
# funded from the faucet and then submit SIGNED on-chain stake transactions,
# which the bootnode mines. Signatures are produced at runtime with the
# locally built `hikma-wallet` so they always match the current signing
# scheme. Dev keys (1..5) are for LOCAL TESTING ONLY.

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
ADMIN_TOKEN="${ADMIN_TOKEN:-local-admin}"
P2P_TOKEN="${P2P_TOKEN:-local-testnet}"
BOOT="http://localhost:3000"

WALLET="./target/release/hikma-wallet"
if [[ ! -x "${WALLET}" ]]; then
  echo "Building hikma-wallet..."
  cargo build --release --bin hikma-wallet
fi

echo "Starting Hikmalayer testnet..."
docker compose -f "${COMPOSE_FILE}" up -d --build

echo "Waiting for the bootnode to boot..."
for _ in $(seq 1 30); do
  if curl -sf "${BOOT}/blockchain/stats" >/dev/null 2>&1; then break; fi
  sleep 1
done

dev_privkey() { printf "%064x" "$1"; }

pubkey_of() { "${WALLET}" sign-stake x 1 1 "$1" | awk '$1=="public_key:"{print $2}'; }
address_of() { "${WALLET}" address "$1" | awk '{print $2}'; }

echo "Registering validators 2..5 on-chain via the bootnode..."
for i in 2 3 4 5; do
  sk=$(dev_privkey "$i")
  pub=$(pubkey_of "$sk")
  addr=$(address_of "$pub")

  # Fund from the faucet (treasury transfer) and mine it in.
  curl -s -X POST "${BOOT}/tokens/faucet" \
    -H "Content-Type: application/json" \
    -H "x-admin-token: ${ADMIN_TOKEN}" \
    -d "{\"to\":\"${addr}\",\"amount\":300}" >/dev/null
  curl -s -X POST "${BOOT}/mine" >/dev/null

  # Sign and submit the on-chain stake (binds the VRF key), then mine it in.
  nonce=$(curl -s "${BOOT}/tokens/nonce/${addr}" | sed 's/.*"next_nonce":\([0-9]*\).*/\1/')
  stake_out=$("${WALLET}" sign-stake "${addr}" 100 "${nonce}" "${sk}")
  sig=$(echo "${stake_out}" | awk '/signature:/{print $2}')
  vrf_pub=$(echo "${stake_out}" | awk '/vrf_public_key:/{print $2}')
  curl -s -X POST "${BOOT}/staking/deposit" \
    -H "Content-Type: application/json" \
    -d "{\"address\":\"${addr}\",\"amount\":100,\"public_key\":\"${pub}\",\"vrf_public_key\":\"${vrf_pub}\",\"nonce\":${nonce},\"signature\":\"${sig}\"}" >/dev/null
  curl -s -X POST "${BOOT}/mine" >/dev/null

  echo "  registered ${addr} (stake 100)"
done

echo "Registering peer mesh (bootnode → validators)..."
for peer in http://validator1:3000 http://validator2:3000 http://validator3:3000 http://validator4:3000 http://rpc:3000; do
  curl -s -X POST "${BOOT}/p2p/peers/register" \
    -H "Content-Type: application/json" \
    -H "x-p2p-token: ${P2P_TOKEN}" \
    -d "{\"address\":\"${peer}\"}" >/dev/null || true
done

echo "Testnet started. Validator set:"
curl -s "${BOOT}/staking/validators"
echo
echo "State: $(curl -s "${BOOT}/blockchain/state")"
