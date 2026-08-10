# Hikmalayer Core

**A sovereign quantum dual-hybrid Layer 1 blockchain, written in Rust.**

Hikmalayer is *dual-hybrid* in two independent senses:

1. **Hybrid consensus** — stake-weighted VRF selection picks each block's leader
   (Proof of Stake); that leader then finalizes the block with Proof of Work.
   Neither stake alone nor hashpower alone produces a block.
2. **Hybrid cryptography** — an account can be *quantum-ready*, authorized by
   **two** signatures over the same message: secp256k1 ECDSA **and** ML-DSA-65
   (FIPS 204, NIST post-quantum category 3). Forging one transaction means
   breaking **both** schemes.

It has its own address format and signing domain — no dependency on Ethereum or
any other chain's conventions.

| | |
|---|---|
| **Native coin** | **HKM** — pays every fee, secures the chain through staking, and is what block rewards are paid in |
| **Native token standard** | **HTS** — fungible assets as consensus objects. Fixed supply at creation, no mint function, reducible only by burning |
| **Native exchange** | Constant-product **AMM** in the state machine. 0.30% fee to liquidity providers |
| **Account types** | `hkm…` classical (ECDSA) · `hkq…` quantum-ready (ECDSA **+** ML-DSA-65) |
| **Flagship application** | **Proof-of-Credential** — hash-only on-chain credentials, verifiable against the block-committed state root |

**Three things it deliberately is not:**

- **No virtual machine.** No user-deployed contracts. Every capability is a
  protocol feature (see [Protocol-native capabilities](#protocol-native-capabilities)).
- **No bridge.** No wrapped or external asset exists here, and none is planned
  ([reasoning](docs/bridge_design.md)).
- **No custody of user keys.** The node has no endpoint that accepts a private
  key, for any purpose.

---

## Documentation

| Document | What it covers |
|---|---|
| [Whitepaper](docs/Whitepaper.md) · [short version](docs/whitepaper_short_version.md) | The system as implemented |
| [Quantum readiness](docs/quantum_readiness.md) | The dual-hybrid signature scheme — and what it does *not* cover |
| [Security assessment](docs/security_assessment.md) | 13 findings, all fixed, each with a regression test |
| [Threat model](docs/threat_model.md) | Adversaries, mitigations, and what is explicitly out of scope |
| [HKM, HTS and listings](docs/hts_and_listings.md) | What the token layer is, and honest expectations about exchanges |
| [API](docs/API.md) · [OpenAPI 3.1](docs/openapi.yaml) · [SDK](sdk/README.md) | Building on it |
| [Consensus flow](docs/consensus_flow.md) · [Validator lifecycle](docs/validator_lifecycle.md) · [Key management](docs/key_management.md) | Running a validator |
| [Wallet security](docs/wallet_security.md) · [Deployment](docs/deployment_guide.md) | Operating it |
| [Mainnet readiness](docs/mainnet_readiness.md) · [External audit guide](docs/external_audit_guide.md) | What remains before launch |
| [Bridge design](docs/bridge_design.md) | Why there is no bridge |

---

## Quick start

```bash
# A complete local chain — treasury funded, validator registered, blocks sealing.
ops/devnet.sh

# In another shell
curl -s http://127.0.0.1:3000/blockchain/stats
cd sdk && HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
```

Or from scratch:

```bash
cargo build --release                       # node + hikma-wallet CLI
./target/release/hikma-wallet keygen        # both identities from one secret
./target/release/hikmalayer                 # run a node
```

---

## Consensus

Stake decides *who* may produce a block; work decides *that* it was produced.

```
   parent block
        │
        ├─► randomness beacon ──► slot input for height N, round R
        │                              │
        │                              ▼
        │                    stake-weighted VRF selection over the
        │                    ON-CHAIN validator set at the PARENT state
        │                              │
        │                              ▼
        │                     the selected leader:
        │                       1. builds the block
        │                       2. proves the VRF for this slot
        │                       3. mines it to the current difficulty  ◄── PoW
        │                       4. signs the block hash (ECDSA)
        │                       5. …and with ML-DSA-65 if it is a hkq account
        ▼                              ▼
   every node re-checks:  selection · VRF proof · PoW · signature(s)
                          · timestamp bounds · state root by RE-EXECUTION
```

- **Unbiasable randomness.** Every block carries an sr25519 VRF proof. A VRF
  output is unique for a given (key, slot), so a validator has nothing to grind.
  Outputs fold into an on-chain beacon that seeds subsequent selection.
- **Liveness rotation.** Round 0's leader is the primary; each elapsed 30-second
  slot timeout opens the next round's leader as a fallback. An offline validator
  delays the chain by at most one timeout — it can never stall it.
- **Sovereign finality.** Finalized blocks are irreversible. A fork must carry
  *more validator-sealed blocks* to displace the local chain; cumulative work
  only breaks exact ties. **Hashrate without stake produces nothing and reorgs
  nothing.**
- **Replay under local rules.** An adopted chain is re-executed under *local*
  network parameters and its state rebuilt from genesis. A candidate's claims
  about its own genesis are never trusted.
- **Slashing.** Equivocation proofs are permissionless and burn the offender's
  stake on chain. Withdrawn stake stays locked and slashable for the unbonding
  period, so misbehaving stake cannot exit ahead of its punishment.

Full detail: [`docs/consensus_flow.md`](docs/consensus_flow.md).

---

## Quantum readiness

**The problem, stated honestly.** secp256k1 falls to Shor's algorithm, and
Hikmalayer's exposure is *worse* than Bitcoin's: a public key is published with
**every** transaction, and a validator's key sits in the on-chain staker set for
as long as its stake does. "Harvest now, decrypt later" applies today.

**The answer.** Two account types on one chain:

```
hkm…  classical      address = SHA256(secp_pub)[..20]                → ECDSA
hkq…  quantum-ready  address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
                                                                     → ECDSA AND ML-DSA-65
```

Both signatures are required, so the account is safe while **either** scheme
holds. The address commits to **both** public keys — without that, an attacker
who broke secp256k1 could pair the victim's classical key with an ML-DSA key of
their own, and the "hybrid" account would fall to a single break.

| Path | Classical | Quantum-ready |
|---|---|---|
| Transfers, HTS tokens, AMM, vesting, credentials | ECDSA | ECDSA **+ ML-DSA-65** |
| Staking | ECDSA | Both; the ML-DSA key is registered on chain |
| **Unbonding** | ECDSA vs. the on-chain key | **Both**, vs. the on-chain keys |
| **Block production** | ECDSA over the block hash | **Both** over the block hash |

The **address** decides which scheme applies, never the transaction — otherwise
an attacker could downgrade a hybrid account by omitting the post-quantum half.
Symmetrically, a classical transaction carrying post-quantum fields is rejected,
so one authorization has exactly one valid encoding.

**Cost:** ~5.4 KB per transaction against ~130 bytes, ~11 ms to sign against
under 1 ms. That is the real price of post-quantum signatures today, and it is
why hybrid is **opt-in per account**. `GENESIS_REQUIRE_HYBRID=1` makes a whole
network quantum-ready-only, committed to by the genesis state root.

**Not covered:** the sr25519 VRF used for leader election is still classical. A
quantum adversary could *predict* a validator's slots — not forge blocks or
spend, since block signatures are separately hybrid. No standardized
post-quantum VRF exists yet.

Full detail, including migration: [`docs/quantum_readiness.md`](docs/quantum_readiness.md).

---

## Protocol-native capabilities

There is **no virtual machine**. Every capability is a transaction type the state
machine executes directly.

The trade is deliberate: the majority of value ever stolen from blockchains was
stolen through contract bugs rather than consensus bugs, and this design has no
contracts to be buggy. The cost is equally real — you cannot write an arbitrary
application that runs on this chain.

| Capability | Consensus objects | Transaction types |
|---|---|---|
| Native value | balances, nonces | `Transfer` |
| Staking | `StakeInfo` (stake, secp256k1 key, VRF key, ML-DSA key) | `Stake`, `Withdraw`, `Slash` |
| Token standard (HTS) | `TokenInfo`, per-token balances | `TokenCreate`, `TokenTransfer`, `TokenBurn` |
| Exchange (AMM) | `AmmPool` (reserves, LP shares) | `AddLiquidity`, `RemoveLiquidity`, `Swap` |
| Vesting | cliff + linear schedules | `Vest` |
| Credentials | credential records | `Certificate` (issue / revoke) |
| Block economics | fee pot, emission schedule | `Reward` |

### Proof-of-Credential

Verifiable credentials as first-class consensus objects. **Only a document hash
goes on chain**, so publishing a credential does not publish its contents.
Revocation is a first-class signed operation, not a convention. Verification
needs no trusted node — a verifier checks the record against the state root
committed in a block.

```bash
# 1. Issue: only the DOCUMENT HASH is published
hikma-wallet sign-credential <id> <subject> <sha256-of-document> false <nonce> <key>
curl -X POST "$NODE/credentials/issue" -H 'content-type: application/json' -d @cred.json

# 2. Verify — returns { credential, height, state_root, block_hash }
curl -s "$NODE/credentials/verify/<id>"

# 3. Revoke: issuer-only, instant, on-chain
hikma-wallet sign-credential <id> <subject> <hash> true <nonce> <key>
```

### HTS — the native token standard

- Deterministic id: `hkt` + hex(SHA-256(`creator:symbol:nonce`)[..20])
- **Fixed supply at creation.** There is no mint operation; supply moves only
  downward, through burning
- Up to 18 decimals, declared at creation and immutable

Because there is no contract, no HTS token can have a hidden mint function, a
blacklist, a transfer hook, or an upgradeable proxy — those properties hold for
*every* HTS token by consensus, not by an audit of one token's code. The same
absence means an HTS token cannot have legitimate custom behaviour either.

### The native AMM

Constant-product (`x·y = k`) pools pairing HKM with any HTS token. A 0.30% fee
stays in the reserves and accrues to liquidity providers. `MINIMUM_LIQUIDITY`
(1,000 shares) is locked permanently on a pool's first deposit, closing the
first-depositor share-inflation attack. Slippage bounds are protocol fields, and
the SDK will not build an unbounded trade.

---

## Economics

| Parameter | Value |
|---|---|
| Decimals | 6 (1 HKM = 1,000,000 base units) |
| Total supply | ~100B HKM — 30B genesis allocation, ~70B mined |
| Initial block reward | 3,700 HKM |
| Halving interval | 9,500,000 blocks (~4.5 years at 15s targets) |
| Tail emission | 50 HKM/block, perpetual — the long-run security budget |
| Transaction fee | Dynamic base fee (EIP-1559-style), floor 0.001 HKM, cap 100 HKM, paid to the block validator |
| Validator minimum | 10,000 HKM |
| Unbonding period | 20 blocks, slashable throughout |
| Target block time | 15s, retargeted every 10 blocks |

The base fee lives in the state root and is recomputed identically by every
node. Emission is consensus-verified per height — a block claiming the wrong
reward is invalid.

---

## Security model

- **Signature-authorized, not session-authorized.** There is no login. The node
  reconstructs each canonical message from the request fields and verifies
  against it, so a request the signature does not cover is not a request.
- **Native identity.** `hkm…`/`hkq…` addresses and a native signing domain.
  Messages are scoped to a **chain id**, so a signature made for one network is
  inert on every other — enforced on both the submission and block-validation
  paths.
- **One authorization, one encoding.** Public keys are accepted in exactly one
  canonical form, closing the transaction malleability that would otherwise let
  a relay re-encode a transaction in flight and produce a different, equally
  valid id.
- **Checked arithmetic everywhere**, with `overflow-checks` on in release.
- **Recipients are validated.** A transfer to a malformed address is rejected
  rather than executed — there is no checksum and no recovery.
- **Deny-by-default authorization.** Admin and P2P endpoints require their
  tokens; an **unset token disables the endpoint** rather than opening it.
  Rotation via `*_TOKEN_CURRENT`/`*_TOKEN_PREVIOUS`, compared in constant time.
- **Cryptographic node identity.** Every gossip envelope is signed by the
  sender's node key and bound to its derived node id;
  `P2P_REQUIRE_IDENTITY=true` rejects unsigned envelopes. Peer reputation
  scoring auto-bans repeat offenders; `P2P_ALLOWLIST` can restrict participation.
- **Bounded resources.** Mempool capped at 1,000 transactions, 100 per block,
  1 MiB request bodies, difficulty clamped 1–5.
- **Atomic persistence.** State is written to a temp file and renamed, so a
  crash mid-write cannot corrupt it.

**Stated limits:** admin tokens are bearer credentials, not signatures. Per-IP
rate limiting is not implemented — deploy behind a proxy that provides it.
Transaction ordering within a block is chosen by its producer, as on every
blockchain; Hikmalayer claims no front-running immunity, which is why slippage
bounds are mandatory in practice.

---

## Wallets and tooling

| Tool | Use |
|---|---|
| **`hikma-wallet` CLI** | Offline keygen and signing. The right choice for treasury, genesis and validator keys |
| **Browser extension** (MV3) | Keys live in the extension's own context, where a website — or an XSS in one — cannot reach them. Per-origin permissions, extension-side approvals |
| **In-page wallet** | AES-256-GCM vault under PBKDF2-SHA256 (310k iterations); unlocked key held under a non-extractable WebCrypto key and decrypted only for the instant of signing, then wiped. Auto-locks on idle and tab close |
| **`@hikmalayer/sdk`** | `LocalSigner`, `HybridSigner`, `ExtensionSigner`; every write builds its canonical message from the same values it puts on the wire |
| **React dashboard** | Swap, liquidity, asset explorer, credentials, explorer — with a Classical / Quantum-ready switch |

Both browser wallets derive the hybrid identity on unlock and hold it **in
memory only**. Nothing extra is written to disk.

Every signature requires explicit approval showing the exact canonical message,
so a scripted signing spree is visible and refusable. Signatures are
byte-identical to the Rust signer — asserted by test, not assumed.

See [`docs/wallet_security.md`](docs/wallet_security.md) and
[`docs/key_management.md`](docs/key_management.md).

---

## Building on Hikmalayer

```js
import { HikmalayerClient, HybridSigner, parseUnits } from "@hikmalayer/sdk";

// Quantum-ready account — identical API, two signatures on the wire
const client = HikmalayerClient.withHybridPrivateKey(process.env.HIKMALAYER_KEY, {
  url: "http://127.0.0.1:3000",
});

await client.transfer({ to: "hkq…", amount: parseUnits("1.5") });
await client.createAsset({ symbol: "LUNA", decimals: 6, initialSupply: parseUnits("1000000") });
await client.addLiquidity({ tokenId, amountHkm: parseUnits("100"), amountToken: parseUnits("400") });
await client.swap({ tokenId, hkmToToken: true, amountIn: parseUnits("5") });   // slippage-bounded
```

Amounts are `BigInt` base units throughout: a JSON number cannot represent the
full supply exactly, and the signature covers the exact digits.

A worked end-to-end application lives in
[`examples/token-launch/launch.mjs`](examples/token-launch/launch.mjs) — it
issues a token, seeds a pool, trades, vests a team allocation, and checks the
SDK's predictions against the chain at every step.

---

## Running a node

```bash
# 1. Generate an identity offline — keys never leave your machine.
#    Prints the private key, both public keys, and BOTH addresses.
hikma-wallet keygen

# 2. Run a validator. Genesis parameters must be identical across a network:
#    they are committed to by the genesis state root.
GENESIS_CHAIN_ID=hikmalayer-testnet \
GENESIS_TREASURY_ADDRESS=hkm… \
GENESIS_VALIDATOR_PUBLIC_KEY=04… \
GENESIS_VALIDATOR_VRF_PUBLIC_KEY=… \
ADMIN_TOKEN=… P2P_TOKEN=… \
VALIDATOR_PRIVATE_KEY=… \
  ./hikmalayer

# 3. Stake to join the validator set (signed offline, executes on-chain)
hikma-wallet sign-stake <address> <amount> <nonce> <private_key>
curl -X POST "$NODE/staking/deposit" -H 'content-type: application/json' -d @stake.json

# 4. Inspect the replicated state
curl -s "$NODE/blockchain/state"        # height, state root, supply, validators
curl -s "$NODE/staking/validators"
```

### Genesis environment

| Variable | Meaning |
|---|---|
| `GENESIS_CHAIN_ID` | The network's name — inside every signed message. A testnet and a mainnet **must** differ |
| `GENESIS_TREASURY_ADDRESS` | Holds the initial supply; seated as the bootstrap validator when its keys are given |
| `GENESIS_VALIDATOR_PUBLIC_KEY` | Canonical uncompressed secp256k1 key |
| `GENESIS_VALIDATOR_VRF_PUBLIC_KEY` | sr25519 VRF key |
| `GENESIS_VALIDATOR_PQ_PUBLIC_KEY` | **Required if the treasury is a `hkq…` address.** Without a key that derives to it, the treasury is not seated as a validator at all |
| `GENESIS_SUPPLY` | Initial supply in base units |
| `GENESIS_VALIDATOR_ALLOWLIST` | Addresses permitted to *join* the validator set (launch posture) |
| `GENESIS_REQUIRE_HYBRID` | `1` accepts only quantum-ready senders network-wide |

### Node environment

| Variable | Purpose |
|---|---|
| `ADMIN_TOKEN` / `ADMIN_TOKEN_CURRENT` / `ADMIN_TOKEN_PREVIOUS` | Admin endpoints. **Unset disables them** |
| `P2P_TOKEN` / `P2P_TOKEN_CURRENT` / `P2P_TOKEN_PREVIOUS` | Peer endpoints, same rule |
| `P2P_REQUIRE_IDENTITY` | `true` rejects unsigned gossip envelopes |
| `P2P_ALLOWLIST` | Restrict participation to named node ids |
| `VALIDATOR_PRIVATE_KEY` | This node's own validator key. Never a foreign key |
| `TREASURY_PRIVATE_KEY` | Enables the devnet faucet. Development only |
| `CORS_ALLOWED_ORIGINS` | Browser origins permitted to call the API |
| `HIKMALAYER_CHECKPOINT` | Boot from a self-verifying checkpoint bundle instead of full replay |

Full deployment guidance: [`docs/deployment_guide.md`](docs/deployment_guide.md).

---

## Testing

```bash
cargo test                             # 139 unit tests
cargo test --release                   # and in the profile validators run
cargo test --release --test security   # 40 adversarial tests
cargo clippy --all-targets -- -D warnings

cd sdk && npm test                     # 58 offline, incl. Rust↔JS byte parity

ops/devnet.sh &                        # a live chain
node examples/token-launch/launch.mjs  # 16-check end-to-end application
cd sdk && HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration   # 21 live
```

[`tests/security.rs`](tests/security.rs) is the adversarial suite: each test
plays an attacker with a specific goal — mint supply, spend someone else's
funds, replay a signature, drain a pool, downgrade a hybrid account — and
asserts the chain refuses. The quantum tests assume the attacker **already holds
the victim's secp256k1 private key**.

Multi-node deployment for manual QA:

```bash
docker compose up -d --build     # bootnode + validators + RPC + Prometheus + Grafana
ops/start_testnet.sh             # or a local multi-node testnet
```

---

## Repository layout

```
hikmalayer-core/
├── src/
│   ├── api/            REST API, request types, block production
│   ├── auth/           admin/P2P tokens, signature auth
│   ├── bin/            hikma-wallet CLI, mint_token
│   ├── blockchain/     block, chain, state machine, transactions
│   ├── consensus/      pos · pow · vrf · pq (ML-DSA-65) · hybrid
│   ├── contract/       credential registry
│   ├── p2p/            protocol envelopes, peerbook, gossip service
│   ├── governance.rs   runtime parameters
│   └── persistence.rs  atomic state persistence
├── tests/security.rs   adversarial suite
├── sdk/                @hikmalayer/sdk — client library + tests
├── dashboard/          React DEX + wallet dashboard
├── extension/          MV3 browser wallet
├── examples/           worked end-to-end applications
├── ops/                devnet, testnet, benchmark scripts
├── docs/               whitepaper, security, protocol and operations docs
└── bench/              benchmark harness and results
```

---

## Project status

**Built and tested:** replicated state machine with per-block state roots ·
on-chain validator set · hybrid PoS/PoW consensus · VRF randomness beacon ·
sovereign finality and fork choice · slashing with unbonding and slashing
windows · calibrated emission and dynamic fees · on-chain vesting ·
Proof-of-Credential · HTS token standard · native AMM DEX · quantum-ready hybrid
accounts across every authorization path · P2P identity, peer scoring and
checkpoint fast-sync · browser wallet, MV3 extension, SDK, OpenAPI spec and
dashboard.

**Before mainnet** — see [`docs/mainnet_readiness.md`](docs/mainnet_readiness.md):

1. **External security audit + adversarial testnet.** The hard gate. This
   cannot be self-performed; the step-by-step engagement guide is in
   [`docs/external_audit_guide.md`](docs/external_audit_guide.md). Post-quantum
   expertise is required in scope, not optional.
2. **Genesis distribution policy** — who receives what from the genesis
   treasury, published as on-chain vesting schedules. A business decision, not
   code.
3. **Production key management** — HSM or remote signer for validators. Note
   that most HSMs cannot produce ML-DSA-65 signatures today, so a hybrid
   validator's key is currently a software key.
4. **Post-quantum leader election** — the sr25519 VRF is still classical,
   pending a standardized post-quantum VRF.

**Not pursued:** a cross-chain bridge. Hikmalayer will not custody external
assets ([reasoning](docs/bridge_design.md)).

---

## Licence

- **[HikmaLayer Business Source License 1.1](LICENSE)** — protocol source code
- **[Contributor License Agreement](CLA.md)** — incoming contributions
- **Whitepaper** — Creative Commons Attribution 4.0 International (CC BY 4.0)

Hikmalayer is developed by Muhammad Ayan Rao, Founder and Director of
Bestower Labs Limited.
