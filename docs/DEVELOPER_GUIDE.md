# Hikma Layer — Developer Integration Guide

**Status: mainnet, unaudited, early-stage.** Confirm current network status before building anything that touches real value.

## 1. What you're connecting to

| | |
|---|---|
| Node URL | `https://node.hikmalayer.com` |
| Dashboard | `https://dashboard.hikmalayer.com` |
| Chain ID | `hikmalayer-mainnet` |
| Block time | ~2 minutes |
| Consensus | Hybrid PoS + PoW — stake decides who may propose; work decides when it settles |

Every request needs the exact `chain_id` above. A transaction signed for any other network — including a local devnet or testnet — is rejected outright by design, so signatures can never be replayed across networks.

## 2. Account model

Account-based, not UTXO. Every account has a balance and a strictly-increasing nonce; there is no unspent-output set to manage.

| Address format | Signing | Notes |
|---|---|---|
| `hkm...` | secp256k1 (ECDSA) | Standard account, same curve as Bitcoin/Ethereum |
| `hkq...` | secp256k1 + ML-DSA-65 | Hybrid, quantum-ready account. Both signatures must verify; the address itself commits to both public keys |

Generate a key pair with the CLI wallet tool:

```bash
./hikma-wallet keygen
```

This prints a private key (keep offline), a public key, a VRF public key, and the derived address. For a hybrid account, add `HIKMALAYER_HYBRID=1` when signing.

## 3. Constructing and sending a transaction

1. Fetch the account's current nonce:
cat > docs/DEVELOPER_GUIDE.md << 'HIKMADOC'
# Hikma Layer — Developer Integration Guide

**Status: mainnet, unaudited, early-stage.** Confirm current network status before building anything that touches real value.

## 1. What you're connecting to

| | |
|---|---|
| Node URL | `https://node.hikmalayer.com` |
| Dashboard | `https://dashboard.hikmalayer.com` |
| Chain ID | `hikmalayer-mainnet` |
| Block time | ~2 minutes |
| Consensus | Hybrid PoS + PoW — stake decides who may propose; work decides when it settles |

Every request needs the exact `chain_id` above. A transaction signed for any other network — including a local devnet or testnet — is rejected outright by design, so signatures can never be replayed across networks.

## 2. Account model

Account-based, not UTXO. Every account has a balance and a strictly-increasing nonce; there is no unspent-output set to manage.

| Address format | Signing | Notes |
|---|---|---|
| `hkm...` | secp256k1 (ECDSA) | Standard account, same curve as Bitcoin/Ethereum |
| `hkq...` | secp256k1 + ML-DSA-65 | Hybrid, quantum-ready account. Both signatures must verify; the address itself commits to both public keys |

Generate a key pair with the CLI wallet tool:

```bash
./hikma-wallet keygen
```

This prints a private key (keep offline), a public key, a VRF public key, and the derived address. For a hybrid account, add `HIKMALAYER_HYBRID=1` when signing.

## 3. Constructing and sending a transaction

1. Fetch the account's current nonce:

[200~
2. Build the transaction:
```json
   {
     "id": "<uuid>",
     "from": "hkm...",
     "to": "hkm...",
     "amount": 1000000,
     "transaction_type": "Transfer",
     "nonce": 4,
     "public_key": "<hex, uncompressed secp256k1>",
     "signature": "<hex, produced in step 3>",
     "chain_id": "hikmalayer-mainnet"
   }
```

3. Sign the canonical message locally. The private key never leaves your machine — nothing is sent to the node until after signing.

4. Submit:~

5. Read the response:
```json
   { "status": "success", "message": "...queued; it executes when mined" }
```

The API is binary at submission — success (queued) or error, with a specific reason. There is no separate "broadcasting" or "pending" push notification; a client polls to detect confirmation (see §5).

## 4. Full endpoint reference

### Tokens & assets

| Endpoint | Purpose |
|---|---|
| `POST /tokens/transfer` | Send HKM |
| `GET /tokens/balance/{account}` | HKM balance |
| `GET /tokens/nonce/{account}` | Next nonce for signing |
| `POST /assets/create` | Mint a new HTS token |
| `POST /assets/transfer` / `POST /assets/burn` | Move or destroy token units |
| `GET /assets` | List all HTS tokens (empty until one is minted) |

### DEX (native AMM)

| Endpoint | Purpose |
|---|---|
| `POST /dex/add` / `POST /dex/remove` | Add or remove liquidity |
| `POST /dex/swap` | Execute a swap |
| `GET /dex/pool/{token_id}` | Pool reserves |
| `GET /dex/quote/{token_id}/{direction}/{amount_in}` | Exact quote before signing |

### Staking, credentials, vesting, governance

| Endpoint | Purpose |
|---|---|
| `POST /staking/deposit` / `POST /staking/withdraw` | Stake or unstake HKM |
| `GET /staking/validators` | Current validator set |
| `POST /credentials/issue` / `POST /credentials/revoke` | Verifiable credentials — hash only, document stays private |
| `GET /credentials/{id}/proof` | Cryptographic proof of a credential |
| `GET /vesting/{address}` | Vesting schedule lookup |
| `GET /governance/config` | Current network parameters |

### Chain state

| Endpoint | Purpose |
|---|---|
| `GET /blockchain/stats` | Height, difficulty, validity, latest hash |
| `GET /blockchain/state` | Chain ID, state root, total supply |
| `GET /blockchain/validate` | Full chain validation, not just a cached flag |
| `GET /explorer/overview` | Same data, explorer-shaped |
| `GET /explorer/search/{query}` | Search by block index, hash, tx id, or address |

## 5. Confirming a transaction

There is no webhook or push notification. Poll:

A submitted transaction appears here while queued. Its disappearance — combined with a rising block height — means it was mined. There is no expiry mechanism; a transaction with a valid nonce and signature waits indefinitely if the network is quiet, rather than timing out.

## 6. What exists today vs. what doesn't

| Component | Status |
|---|---|
| Wallet, signing, transfers | Live and working |
| Native DEX / AMM logic | Live and working, but zero pools exist — nothing has been minted yet |
| HTS tokens | The system works; none have been created on mainnet yet |
| Staking | Live — one validator (the bootnode) currently |
| Verifiable credentials | Live and demonstrated end to end |
| Fiat on-ramp | Does not exist. There is no way to buy HKM with fiat currency today |
| Price oracle | Does not exist. No fiat exchange rate is available anywhere in the system |

Build against what's real. A DEX integration will correctly return "no pool" for every token right now — that's accurate, not a bug.

## 7. Security notes

- Every transaction is scoped to `chain_id` — a signature made for a different network (a local testnet, for instance) is inert here
- Signatures are verified before any state is read — an invalid signature never touches account balances
- Hybrid (`hkq`) accounts require both signatures to verify independently; compromising one scheme alone is not sufficient to spend
- The network is not yet independently audited. Treat it accordingly for anything beyond testing

---

*Bestower Labs Limited — Confidential — Internal Use Only*
