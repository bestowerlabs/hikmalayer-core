#!/usr/bin/env bash
set -euo pipefail

# Boots the local Docker testnet and seeds an identical validator set on
# every node using SIGNED staking requests. The dev keys (1..5) and their
# precomputed stake signatures below are for local testing only.
#
# Signatures were produced with:
#   hikma-wallet sign-stake <address> 100 1 <dev_private_key>

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
ADMIN_TOKEN="${ADMIN_TOKEN:-local-admin}"
P2P_TOKEN="${P2P_TOKEN:-local-testnet}"

echo "Starting Hikmalayer testnet..."
docker compose -f "${COMPOSE_FILE}" up -d --build

echo "Waiting for nodes to boot..."
sleep 5

# address|public_key|stake_signature (dev keys 1..5, stake 100, nonce 1)
VALIDATORS=(
  "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf|0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8|2d11170678f37d338e2c87ac5977f2f1174be7d04f9ceda885a54a9b118f5cc4184a3e0485924a9b903937e21adffe226d1cf85f6a8e8b3b705fbaa543a35211"
  "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf|04c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee51ae168fea63dc339a3c58419466ceaeef7f632653266d0e1236431a950cfe52a|607af91ac11d911709b2a6a0fb649bcd56187ad57f2f8382d860234a9383441d6a38f90065ed7a1f2e84fc7af7912934088f03d1e7bd5a46fd7b666ca3fb1b5f"
  "0x6813eb9362372eef6200f3b1dbc3f819671cba69|04f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9388f7b0f632de8140fe337e62a37f3566500a99934c2231b6cb9fd7584b8e672|b28984daa5a3c0e8b017005d219ee629af5c3280085b683538232a12cf0dcfb436e8c44c0ccf0bf90253972f4f7a94328cec6622864e84c5618feab326d67da7"
  "0x1eff47bc3a10a45d4b230b5d10e37751fe6aa718|04e493dbf1c10d80f3581e4904930b1404cc6c13900ee0758474fa94abe8c4cd1351ed993ea0d455b75642e2098ea51448d967ae33bfbdfe40cfe97bdc47739922|06458ba6b582d3e7fcbf7a2b1ce79b19a3969b819a8b3a1f89598b378c8f25e07c1a7c5a4b129dc7a6dff2bf4dc31ffe04a24943c8c2c98faf8e866b2f6323d1"
  "0xe1ab8145f7e55dc933d51a18c793f901a3a0b276|042f8bde4d1a07209355b4a7250a5c5128e88b84bddc619ab7cba8d569b240efe4d8ac222636e5e3d6d4dba9dda6c9c426f788271bab0d6840dca87d3aa6ac62d6|fa25086fc4af7e4925eba74cbfcb64ca9791ad66400c8b9865da3bee33acd7483a25c73cc3eba3affe8bbb57e538ab0d265b0b45b30669db5e05d6a8f225851b"
)

NODES=(
  "http://localhost:3000"
  "http://localhost:3001"
  "http://localhost:3002"
  "http://localhost:3003"
  "http://localhost:3004"
  "http://localhost:3010"
)

# Internal docker-network addresses used for peer registration.
PEER_ADDRESSES=(
  "http://bootnode:3000"
  "http://validator1:3000"
  "http://validator2:3000"
  "http://validator3:3000"
  "http://validator4:3000"
  "http://rpc:3000"
)

echo "Seeding identical validator set on every node..."
for node_url in "${NODES[@]}"; do
  for entry in "${VALIDATORS[@]}"; do
    IFS='|' read -r address public_key signature <<<"${entry}"

    # Fund the validator account (admin faucet), then submit its signed stake.
    curl -s -X POST "${node_url}/tokens/faucet" \
      -H "Content-Type: application/json" \
      -H "x-admin-token: ${ADMIN_TOKEN}" \
      -d "{\"to\":\"${address}\",\"amount\":200}" >/dev/null

    curl -s -X POST "${node_url}/staking/deposit" \
      -H "Content-Type: application/json" \
      -d "{\"address\":\"${address}\",\"amount\":100,\"public_key\":\"${public_key}\",\"nonce\":1,\"signature\":\"${signature}\"}" >/dev/null
  done
done

echo "Registering peer mesh..."
for i in "${!NODES[@]}"; do
  for j in "${!PEER_ADDRESSES[@]}"; do
    if [[ "$i" != "$j" ]]; then
      curl -s -X POST "${NODES[$i]}/p2p/peers/register" \
        -H "Content-Type: application/json" \
        -H "x-p2p-token: ${P2P_TOKEN}" \
        -d "{\"address\":\"${PEER_ADDRESSES[$j]}\"}" >/dev/null || true
    fi
  done
done

echo "Testnet started. Validators staked on all nodes; mine via any validator's /mine."
