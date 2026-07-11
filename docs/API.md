# Hikmalayer REST API Documentation

## Overview

Hikmalayer is a hybrid PoS/PoW blockchain platform with REST execution APIs, staking, governance/slashing controls, and a dedicated P2P protocol endpoint for inter-node communication. This documentation provides API integration guidelines for operators and developers.

**Base URL:** `http://127.0.0.1:3000`  
**Version:** 3.2 (state machine + VRF election + dynamic fee market)  
**Protocol:** HTTP/HTTPS  
**Content-Type:** `application/json`

## ⚠️ Consensus v3 changes (breaking)

Where an example below conflicts with this section, this section is
authoritative.

**Native identity — no Ethereum.** Addresses are `hkm` + hex(SHA-256(uncompressed
secp256k1 public key)[..20]) — 43 characters. There is no `0x`/keccak address and
no `personal_sign`/Ethereum message prefix. All signatures are native compact
secp256k1 over the Hikmalayer signing domain. Generate identities and signatures
with `hikma-wallet`.

**On-chain state machine.** Balances, the validator set, and per-account nonces
are chain state — a deterministic function of the blocks. A transfer/stake/withdraw
is *queued* on submission and only takes effect when mined into a block; `GET
/tokens/balance/{account}` reflects on-chain state. `GET /blockchain/state`
returns the height, **state root**, total supply, and validator count.

**Authorization is deny-by-default.** `ADMIN_TOKEN` and `P2P_TOKEN` must be set
on the node; endpoints gated by an unset token reject every request.

- Admin-gated (`x-admin-token`): `/tokens/faucet`, `/certificates/issue`,
  `/certificates/attest`, `/mining/difficulty` (POST), `/governance/config` (POST),
  `/slashing/evidence`.
- P2P-gated (`x-p2p-token`): `/p2p/*` including `GET /p2p/chain`.

**Transfers must be signed.** `POST /tokens/transfer` body:

```json
{
  "from": "hkm…", "to": "hkm…", "amount": 10, "nonce": 1,
  "public_key": "…uncompressed secp256k1 hex…",
  "signature": "…hikma-wallet sign-transfer output…"
}
```

The signature covers `hikmalayer-transfer:{from}:{to}:{amount}:{nonce}`; the
`public_key` must derive to `from`. Fetch the next nonce with
`GET /tokens/nonce/{account}`, sign offline with `hikma-wallet sign-transfer`.

**Staking is on-chain, signed, and key-bound.** `POST /staking/deposit` submits a
signed Stake transaction (`public_key`, `nonce`, signature over
`hikmalayer-stake:{address}:{amount}:{nonce}`; `address` must derive from
`public_key`). `POST /staking/withdraw` submits a signed Withdraw transaction
(signature over `hikmalayer-withdraw:{address}:{amount}:{nonce}` by the
validator's on-chain key). Both take effect when mined; the validator set is
derived from state.

**Faucet.** `POST /tokens/faucet` (admin) is a signed transfer from the treasury
account; it requires the node to hold `TREASURY_PRIVATE_KEY` (dev only).

**Mining.** `POST /mine` produces a block only when the node's own
`VALIDATOR_PRIVATE_KEY` identity is the PoS-selected validator. External
validators use `POST /mine/propose` (returns the PoW-mined unsigned block, whose
`state_root` already reflects execution, plus its hash), sign the hash offline
(`hikma-wallet sign-block`), and submit to `POST /mine/submit`. Every accepted
block mints a **halving** reward to its validator: `reward = BLOCK_REWARD >>
(height / 1_000_000)`, so emission halves every 1,000,000 blocks and trends to
zero. The reward for each height is consensus-verified — no node can mint more
than the schedule allows.

**Slashing.** `POST /slashing/equivocation` is permissionless: submit a
`{ "block_a": <Block>, "block_b": <Block> }` proof that a validator signed two
different blocks at the same height. It becomes an on-chain Slash transaction and
burns the offender's stake when mined.

**Economics (v3.2).** Every Transfer/Stake/Withdraw pays the current **dynamic
base fee** to the block validator (senders need `amount + base_fee`). The base
fee is congestion-responsive (EIP-1559-style, ±12.5%/block toward a 50-tx
target) and lives in the state root, so it is identical on every node. Read it
from `GET /fees`, `GET /tokens/nonce/{account}` (`base_fee` field), or
`GET /blockchain/stats`. Withdrawals unbond for 20
blocks — still slashable — before releasing; inspect with
`GET /staking/unbonding/{address}`. Equivocation proofs are accepted only
within the 20-block slashing window. Difficulty retargets deterministically
every 10 blocks toward a 15s block time; `POST /mining/difficulty` no longer
sets it. Mempool caps: 1,000 pending txs, 100 txs/block, 1 MiB request bodies.

**Fast-sync (checkpoint pruning).** `GET /checkpoint/bundle` (p2p) serves a
self-verifying `CheckpointBundle`: a retarget-boundary anchor block, its full
state, and the forward blocks up to the tip. Start a fresh node with
`HIKMALAYER_CHECKPOINT=<bundle.json>` (fetched from a trusted peer) and it boots
directly from that anchor — no full genesis replay — then reconstructs a
byte-identical state root, randomness beacon, and difficulty before it resumes
mining. The anchor is constrained to a retarget boundary so difficulty math is
exact under pruning, and the anchor's `state_root` must match its embedded state
or import is rejected; every forward block is re-validated against consensus on
load. A persisted local chain always takes precedence over the bundle, so this
only fast-syncs a genuinely fresh node.

**New/changed endpoints:** `GET /blockchain/state`, `POST /slashing/equivocation`,
`POST /tokens/faucet` (admin), `GET /tokens/nonce/{account}`, `POST /mine/propose`,
`POST /mine/submit`, `GET /p2p/chain` (p2p), `GET /checkpoint/bundle` (p2p),
`POST /certificates/attest` (admin).
`POST /certificates/verify` is a read-only lookup. `POST /auth/verify` now also
requires a `public_key` field (native signature).

## Quick Start

### Prerequisites

- Hikmalayer server running on port 3000
- HTTP client (curl, Postman, or any REST client)
- Basic understanding of blockchain concepts

### Authorization Tokens

Hikmalayer uses deny-by-default admin and P2P authorization headers:

- `ADMIN_TOKEN`: admin endpoints (faucet, certificates, difficulty, governance, slashing) require `x-admin-token`. Unset = disabled.
- `P2P_TOKEN`: P2P peer, chain-sync, and block gossip endpoints require `x-p2p-token`. Unset = disabled.

### Getting Started

1. Start the Hikmalayer server: `ADMIN_TOKEN=… P2P_TOKEN=… VALIDATOR_PRIVATE_KEY=… cargo run`
2. Verify the blockchain status: `GET /blockchain/stats`
3. Generate keys offline: `cargo run --bin hikma-wallet keygen`
4. Fund, stake, transfer (signed), and mine blocks

---

## API Endpoints

### 🌐 P2P Protocol (Phase-4)

#### Receive Protocol Envelope

Dedicated inter-node protocol endpoint for envelope-based messages (`Ping`, `PeerAnnounce`, `Block`, `BlockBatch`).

**Endpoint:** `POST /p2p/protocol`

**Headers:**
- `Content-Type: application/json`
- `x-p2p-token: <token>` (required when `P2P_TOKEN` is configured)

**Request Body (example):**

```json
{
  "protocol_version": "hikmalayer-p2p/1",
  "node_id": "validator-1",
  "message_id": "uuid",
  "timestamp": "2026-01-01T00:00:00Z",
  "payload": {
    "type": "Ping"
  }
}
```

**Response:**

```json
{
  "status": "success|error",
  "message": "pong|..."
}
```

## License

Hikmalayer is licensed under the HikmaLayer Business Source License 1.1. See the repository
`LICENSE` file for full terms.

### 🎓 Certificate Management

#### Issue Certificate

Creates a new digital certificate and adds it to pending transactions.

**Endpoint:** `POST /certificates/issue`

**Request Body:**

```json
{
  "id": "string",
  "issued_to": "string",
  "description": "string"
}
```

**Response:**

```json
{
  "status": "success",
  "message": "Certificate {id} issued to {issued_to} and added to pending transactions"
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:3000/certificates/issue \
  -H "Content-Type: application/json" \
  -d '{
    "id": "CERT001",
    "issued_to": "Alice",
    "description": "Blockchain Developer Certificate"
  }'
```

#### Verify Certificate

Validates the existence and authenticity of a certificate.

**Endpoint:** `POST /certificates/verify`

**Request Body:**

```json
{
  "id": "string"
}
```

**Response:**

```json
{
  "status": "success|error",
  "message": "Certificate {id} verified"
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:3000/certificates/verify \
  -H "Content-Type: application/json" \
  -d '{"id": "CERT001"}'
```

---

### 💰 Token Management

#### Transfer Tokens

Transfers tokens between accounts and creates a blockchain transaction.

**Endpoint:** `POST /tokens/transfer`

**Request Body:**

```json
{
  "from": "string",
  "to": "string",
  "amount": number
}
```

**Response:**

```json
{
  "status": "success|error",
  "message": "Transferred {amount} tokens from {from} to {to} and added to blockchain"
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:3000/tokens/transfer \
  -H "Content-Type: application/json" \
  -d '{
    "from": "admin",
    "to": "alice",
    "amount": 100
  }'
```

#### Get Token Balance

Retrieves the token balance for a specific account.

**Endpoint:** `GET /tokens/balance/{account}`

**Parameters:**

- `account` (path): Account identifier

**Response:**

```json
{
  "account": "string",
  "balance": number
}
```

**Example:**

```bash
curl http://127.0.0.1:3000/tokens/balance/alice
```

---

### 📦 Blockchain Operations

#### Get All Blocks

Retrieves all blocks in the blockchain.

**Endpoint:** `GET /blocks`

**Response:**

```json
[
  "Block { index: 0, timestamp: ..., transactions: [...], ... }",
  "Block { index: 1, timestamp: ..., transactions: [...], ... }"
]
```

**Example:**

```bash
curl http://127.0.0.1:3000/blocks
```

#### Get Block by Index

Retrieves a specific block by its index.

**Endpoint:** `GET /blocks/{index}`

**Parameters:**

- `index` (path): Block index (0-based)

**Response:**

```json
"Block { index: 2, timestamp: 2025-08-03T01:07:55.837727800Z, ... }"
```

**Example:**

```bash
curl http://127.0.0.1:3000/blocks/0
```

#### Get Blockchain Statistics

Provides comprehensive blockchain metrics and health status.

**Endpoint:** `GET /blockchain/stats`

**Response:**

```json
{
  "total_blocks": number,
  "pending_transactions": number,
  "difficulty": number,
  "is_valid": boolean,
  "latest_hash": "string",
  "finalized_height": number,
  "finality_depth": number
}
```

**Example:**

```bash
curl http://127.0.0.1:3000/blockchain/stats
```

---

### ⛏️ Mining Operations

#### Mine Block

Processes all pending transactions into a new block using proof-of-work.

**Endpoint:** `POST /mine`

**Response:**

```json
{
  "status": "success|info",
  "message": "Successfully mined block with {count} transactions",
  "block_index": number,
  "transactions_count": number
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:3000/mine
```

#### Get Mining Difficulty

Returns the current proof-of-work difficulty level.

**Endpoint:** `GET /mining/difficulty`

**Response:**

```json
{
  "current_difficulty": number
}
```

**Example:**

```bash
curl http://127.0.0.1:3000/mining/difficulty
```

#### Set Mining Difficulty

Updates the mining difficulty for future blocks.

**Endpoint:** `POST /mining/difficulty`

**Request Body:**

```json
{
  "difficulty": number
}
```

**Response:**

```json
{
  "status": "success",
  "message": "Mining difficulty changed from {old} to {new}"
}
```

**Example:**

```bash
curl -X POST http://127.0.0.1:3000/mining/difficulty \
  -H "Content-Type: application/json" \
  -d '{"difficulty": 4}'
```

---

### ✔️ Validation & Security

#### Validate Blockchain

Performs comprehensive blockchain integrity validation.

**Endpoint:** `GET /blockchain/validate`

**Response:**

```json
{
  "is_valid": boolean,
  "message": "string",
  "details": "string"
}
```

**Example:**

```bash
curl http://127.0.0.1:3000/blockchain/validate
```

#### Validate Block

Validates a specific block's integrity and linkage.

**Endpoint:** `GET /blocks/{index}/validate`

**Parameters:**

- `index` (path): Block index to validate

**Response:**

```json
{
  "is_valid": boolean,
  "message": "Block {index} is valid|validation failed",
  "details": "string"
}
```

**Example:**

```bash
curl http://127.0.0.1:3000/blocks/1/validate
```

#### Validate Chain (Legacy)

Legacy endpoint for tutorial compatibility.

**Endpoint:** `GET /validate`

**Response:**

```json
{
  "status": "success|error",
  "message": "Blockchain is valid.|Blockchain is invalid!"
}
```

---

### 📝 Transaction Management

#### Get Pending Transactions

Retrieves all transactions waiting to be mined.

**Endpoint:** `GET /transactions/pending`

**Response:**

```json
[
  "Transaction { id: \"uuid\", from: \"account\", to: \"account\", amount: 100, ... }",
  "Transaction { id: \"uuid\", from: null, to: \"account\", amount: 0, ... }"
]
```

**Example:**

```bash
curl http://127.0.0.1:3000/transactions/pending
```

---

## Integration Examples

### Complete Workflow Example

```bash
# 1. Check initial blockchain status
curl http://127.0.0.1:3000/blockchain/stats

# 2. Issue a certificate
curl -X POST http://127.0.0.1:3000/certificates/issue \
  -H "Content-Type: application/json" \
  -d '{
    "id": "DEV001",
    "issued_to": "developer@company.com",
    "description": "Senior Blockchain Developer Certification"
  }'

# 3. Transfer tokens
curl -X POST http://127.0.0.1:3000/tokens/transfer \
  -H "Content-Type: application/json" \
  -d '{"from": "admin", "to": "developer@company.com", "amount": 500}'

# 4. Check pending transactions
curl http://127.0.0.1:3000/transactions/pending

# 5. Mine the transactions
curl -X POST http://127.0.0.1:3000/mine

# 6. Validate the blockchain
curl http://127.0.0.1:3000/blockchain/validate

# 7. Verify certificate
curl -X POST http://127.0.0.1:3000/certificates/verify \
  -H "Content-Type: application/json" \
  -d '{"id": "DEV001"}'

# 8. Check final balances
curl http://127.0.0.1:3000/tokens/balance/admin
curl http://127.0.0.1:3000/tokens/balance/developer@company.com
```

### JavaScript Integration

```javascript
class HikmalayerClient {
  constructor(baseUrl = "http://127.0.0.1:3000") {
    this.baseUrl = baseUrl;
  }

  async issueCertificate(id, issuedTo, description) {
    const response = await fetch(`${this.baseUrl}/certificates/issue`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, issued_to: issuedTo, description }),
    });
    return response.json();
  }

  async transferTokens(from, to, amount) {
    const response = await fetch(`${this.baseUrl}/tokens/transfer`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ from, to, amount }),
    });
    return response.json();
  }

  async mineBlock() {
    const response = await fetch(`${this.baseUrl}/mine`, { method: "POST" });
    return response.json();
  }

  async getBlockchainStats() {
    const response = await fetch(`${this.baseUrl}/blockchain/stats`);
    return response.json();
  }
}

// Usage
const client = new HikmalayerClient();
await client.issueCertificate("CERT001", "Alice", "Developer Certificate");
await client.mineBlock();
```

### Python Integration

```python
import requests
import json

class HikmalayerClient:
    def __init__(self, base_url='http://127.0.0.1:3000'):
        self.base_url = base_url
        self.headers = {'Content-Type': 'application/json'}

    def issue_certificate(self, cert_id, issued_to, description):
        payload = {
            'id': cert_id,
            'issued_to': issued_to,
            'description': description
        }
        response = requests.post(
            f'{self.base_url}/certificates/issue',
            headers=self.headers,
            data=json.dumps(payload)
        )
        return response.json()

    def transfer_tokens(self, from_account, to_account, amount):
        payload = {'from': from_account, 'to': to_account, 'amount': amount}
        response = requests.post(
            f'{self.base_url}/tokens/transfer',
            headers=self.headers,
            data=json.dumps(payload)
        )
        return response.json()

    def mine_block(self):
        response = requests.post(f'{self.base_url}/mine')
        return response.json()

    def get_blockchain_stats(self):
        response = requests.get(f'{self.base_url}/blockchain/stats')
        return response.json()

# Usage
client = HikmalayerClient()
client.issue_certificate('CERT001', 'Alice', 'Developer Certificate')
client.mine_block()
```

---

## Error Handling

### Common HTTP Status Codes

- `200 OK`: Request successful
- `400 Bad Request`: Invalid request payload
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Server error

### Error Response Format

```json
{
  "status": "error",
  "message": "Descriptive error message"
}
```

### Common Error Scenarios

1. **Insufficient Token Balance**

   ```json
   {
     "status": "error",
     "message": "Failed to transfer tokens from alice to bob"
   }
   ```

2. **Block Not Found**

   ```json
   {
     "is_valid": false,
     "message": "Block not found",
     "details": "Block index 999 does not exist"
   }
   ```

3. **Invalid Certificate**
   ```json
   {
     "status": "error",
     "message": "Failed to verify certificate INVALID001"
   }
   ```

---

## Best Practices

### Transaction Management

- Always check pending transactions before mining
- Validate blockchain integrity after mining operations
- Monitor token balances after transfers

### Mining Operations

- Mine blocks regularly to process pending transactions
- Adjust difficulty based on network requirements
- Validate blocks after mining for integrity

### Certificate Management

- Use unique, descriptive certificate IDs
- Verify certificates immediately after issuance
- Store certificate details securely off-chain if needed

### Security Considerations

- Validate all user inputs before API calls
- Implement rate limiting for production use
- Monitor blockchain validation status regularly
- Use HTTPS in production environments

---

## System Requirements

### Server Requirements

- Rust 1.70+
- Tokio async runtime
- 512MB+ RAM recommended
- 1GB+ disk space for blockchain data

### Client Requirements

- HTTP/1.1 compatible client
- JSON parsing capabilities
- Network connectivity to server

---

## Support & Resources

### Documentation

- [Rust Documentation](https://doc.rust-lang.org/)
- [Axum Framework](https://docs.rs/axum/)
- [Tokio Runtime](https://tokio.rs/)

### Community

- GitHub Issues: Report bugs and feature requests
- API Updates: Check for latest endpoint additions
- Integration Examples: Community-contributed examples

---

**© 2025 Hikmalayer Platform. Built with Rust and Axum.**
