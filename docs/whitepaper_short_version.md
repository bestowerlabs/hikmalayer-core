# Hikmalayer: A Quantum Dual-Hybrid Layer 1 for Digital Credentials and Native Assets

**Version 2.0**  
**August 2026**  
**License:** Creative Commons Attribution 4.0 International (CC BY 4.0)

**Author:** Mr. Muhammad Ayan Rao, Director, Bestower Labs Limited

---

## Abstract

Hikmalayer is a sovereign Layer 1 blockchain, written in Rust, that is
**dual-hybrid in two independent senses**: hybrid *consensus* (stake-weighted
VRF leader selection, then Proof-of-Work finalization by that leader) and
hybrid *cryptography* (an account may require **two** signatures over the same
message — secp256k1 ECDSA **and** ML-DSA-65, FIPS 204 — so forging one
transaction means breaking both schemes).

Its token layer and exchange layer are protocol-native: HKM is the native coin,
HTS tokens and a constant-product automated market maker are consensus objects
executed by the state machine itself. There is **no virtual machine** and **no
bridge**, both by decision. Its flagship application is Proof-of-Credential —
verifiable credentials as first-class consensus objects, where only a document
*hash* is published and any third party can verify against the block-committed
state root without trusting a node.

Hikmalayer is stewarded by Bestower Labs Limited and aligned with enterprise-grade governance and
licensing requirements.

This short whitepaper is distributed under CC BY 4.0 to enable sharing and citation with proper
attribution.

This document describes the system **as implemented**; where something is
planned rather than built, it says so.

**Keywords:** Blockchain, Layer 1, Post-Quantum Cryptography, ML-DSA, FIPS 204,
Hybrid Signatures, Proof-of-Stake, Proof-of-Work, VRF, Verifiable Credentials,
Native Token Standard, Automated Market Maker

---

## 1. Introduction

### 1.1 Problem Statement

Traditional digital credential systems suffer from several critical issues:

- **Manual verification processes** that are slow and error-prone
- **Centralized trust dependencies** creating single points of failure
- **Fraud vulnerability** with easily forged certificates
- **Limited interoperability** between different credentialing systems
- **Complex integration requirements** that limit adoption

### 1.2 Solution Overview

Hikmalayer addresses these challenges through:

- **Hash-only on-chain credentials** — the document never leaves the issuer,
  so publishing a credential does not publish its contents
- **Verification against the state root**, so no verifier has to trust a node
- **First-class revocation**, signed by the issuer, effective immediately
- **Protocol-native tokens and exchange** (HTS + AMM) with no contract to audit
- **Hybrid PoS/PoW consensus** — stake-weighted VRF leader selection, then
  Proof-of-Work finalization by that leader
- **Hybrid post-quantum signatures**, opt-in per account (§5b)
- **Deny-by-default operational authorization** and finality tracking

---

## Licensing

Hikmalayer is licensed under the HikmaLayer Business Source License 1.1. See the repository
`LICENSE` file for full terms and commercial licensing options.

This document is licensed under CC BY 4.0.

Contributor terms are governed by the HikmaLayer CLA in `CLA.md`.

---

## 2. Technical Architecture

### 2.1 System Components

```
┌─────────────────────────────────────────────────────────┐
│  REST API  ·  SDK  ·  browser wallet  ·  extension      │
├─────────────────────────────────────────────────────────┤
│  Protocol-native capabilities                           │
│  HTS tokens · AMM · vesting · credentials · staking     │
│  (consensus objects — there is NO virtual machine)      │
├─────────────────────────────────────────────────────────┤
│  Authorization                                          │
│  secp256k1 ECDSA  ·  ML-DSA-65 for hkq accounts         │
├─────────────────────────────────────────────────────────┤
│  Hybrid PoS/PoW consensus  ·  sr25519 VRF beacon        │
├─────────────────────────────────────────────────────────┤
│  Replicated state machine  ·  state root  ·  P2P        │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Blockchain Layer

```rust
pub struct Block {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub transactions: Vec<String>,
    pub merkle_root: String,             // commits to the exact contents
    pub state_root: String,              // commits to the state AFTER execution
    pub previous_hash: String,
    pub difficulty: usize,
    pub nonce: u64,
    pub hash: String,
    pub validator: Option<String>,
    pub validator_public_key: Option<String>,
    pub validator_signature: Option<String>,
    pub validator_pq_signature: Option<String>,  // hybrid validators
    pub vrf_output: Option<String>,      // randomness beacon contribution
    pub vrf_proof: Option<String>,
}
```

**Key properties:**

- **State root.** Every block commits to the full chain state after executing
  it, so no node can forge a balance, a credential or a validator set without
  every other node detecting it by re-execution.
- **Merkle root** over the transaction payloads.
- **Bounded difficulty (1–5)**, so a malformed value can neither disable
  Proof-of-Work nor stall a node.
- **Fork choice by cumulative work**, with finalized history protected.
  An adopted chain is re-executed under *local* parameters — a candidate's
  claims about its own genesis are never trusted.

### 2.3 Protocol-native capabilities (no VM)

Hikmalayer ships **no virtual machine**: no user-deployable bytecode, no gas
metering. Every capability is a transaction type the state machine executes
directly. The trade is deliberate — the majority of value ever stolen from
blockchains was stolen through contract bugs rather than consensus bugs, and
this design has no contracts to be buggy. The cost is equally real: you cannot
write an arbitrary application that runs on this chain.

**Proof-of-Credential** — the flagship. A credential record holds an issuer, a
subject, a **document hash**, a revocation flag and a height. Issuance and
revocation are both signed transactions; verification returns the credential
together with the height, state root and block hash, so any third party checks
it against consensus rather than against a node's word.

### 2.4 Token and exchange layer

**HKM** is the native coin: it pays fees, secures the chain through staking,
and is the unit block rewards are paid in.

**HTS (Hikmalayer Token Standard)** assets are consensus objects:

- Deterministic id: `hkt` + hex(SHA-256(`creator:symbol:nonce`)[..20])
- **Fixed supply at creation.** There is no mint operation; supply moves only
  downward, through burning. No issuer can inflate a token after the fact.
- Up to 18 decimals, declared at creation and immutable.

Because there is no contract, no HTS token can have a hidden mint function, a
blacklist, a transfer hook, or an upgradeable proxy — those properties hold for
*every* HTS token by consensus, not by an audit of one token's code.

**The AMM** is a constant-product (`x·y = k`) market maker in the state machine,
pairing HKM with any HTS token. A 0.30% fee stays in the reserves and accrues to
liquidity providers; `MINIMUM_LIQUIDITY` is locked permanently on a pool's first
deposit, closing the first-depositor share-inflation attack. Slippage bounds are
protocol fields, and the SDK will not build an unbounded trade.

**There is no bridge**, and none is planned. No wrapped or external asset (BTC,
ETH, USDT, …) is or will be tradeable on this chain.

---

## 3. Consensus Mechanism

### 3.1 Hybrid PoS → PoW

Neither stake alone nor hashpower alone produces a block:

1. **Leader selection (PoS).** A stake-weighted draw over the on-chain
   validator set, seeded by the VRF randomness beacon at the parent state.
2. **Finalization (PoW).** The selected leader — and only that leader — mines
   the block to the current difficulty.
3. **Validation.** Every node re-checks the selection, the VRF proof, the
   Proof-of-Work, both block signatures where applicable, and the state root by
   re-executing the block.

**Liveness rotation.** Round 0's leader is the primary; each elapsed slot
timeout opens the next round's leader as a fallback, so an offline validator
cannot stall the chain. A block must come from the *smallest* open round that
selects its producer, and its VRF must verify against exactly that round's slot
input.

### 3.2 Randomness

Every block carries an sr25519 VRF proof over its slot input. A VRF output is
unique for a given (key, input), so a validator has nothing to grind — it cannot
try candidate values to improve its own selection odds. Outputs fold into an
on-chain beacon that seeds subsequent selection.

### 3.3 Validator accountability

Staking and unbonding are signed on-chain transactions, so the validator set is
derived from state rather than node-local bookkeeping. Provable misbehaviour —
wrong slot, bad signature, bad Proof-of-Work, tampered payload or state root —
is distinguished from structural problems and is slashable. A minimum stake
applies, and withdrawals must either exit fully or leave at least that minimum.

---

## 4. API and Integration

### 4.1 REST Endpoints

**Certificate Management:**

- `POST /certificates/issue` - Issue new certificates
- `POST /certificates/verify` - Verify certificate authenticity

**Token Operations:**

- `POST /tokens/transfer` - Transfer tokens between accounts
- `GET /tokens/balance/{account}` - Check account balance

**Blockchain Access:**

- `GET /blocks` - Retrieve all blocks
- `GET /blockchain/stats` - Get blockchain statistics
- `POST /mine` - Mine pending transactions

**Validation:**

- `GET /blockchain/validate` - Validate entire blockchain
- `GET /blocks/{index}/validate` - Validate specific block

### 4.2 Integration

Every value-bearing call is **signature-authorized**. There is no login: the
node reconstructs the canonical message from the request fields and verifies
the sender's signature against it, so a request the signature does not cover is
not a request. The node **never accepts a private key**.

```js
import { HikmalayerClient, LocalSigner, HybridSigner, parseUnits } from "@hikmalayer/sdk";

// Classical account
const client = HikmalayerClient.withPrivateKey(process.env.HIKMALAYER_KEY);

// Quantum-ready account — identical API, two signatures on the wire
const quantum = HikmalayerClient.withHybridPrivateKey(process.env.HIKMALAYER_KEY);

await client.transfer({ to: "hkq…", amount: parseUnits("1.5") });
await client.swap({ tokenId, hkmToToken: true, amountIn: parseUnits("10") });
```

The SDK builds every canonical message from the same values it puts on the
wire, so a developer cannot sign one amount and submit another. Amounts are
`BigInt` base units throughout, because a JSON number cannot represent the full
supply exactly.

For cold keys, `hikma-wallet` signs offline and only the signature is
submitted:

```bash
hikma-wallet keygen                 # prints both the hkm… and hkq… identities
HIKMALAYER_HYBRID=1 hikma-wallet sign-transfer <from> <to> <amount> <nonce> <key>
```

---

## 5. Use Cases

### 5.1 Academic Credentials

- **Digital Diplomas**: Tamper-proof degree certificates
- **Course Completion**: Verifiable training records
- **Professional Certifications**: Industry-recognized credentials
- **Instant Verification**: Eliminate manual verification delays

### 5.2 Corporate Applications

- **Employee Training**: Track compliance and skill development
- **Vendor Certification**: Verify supplier qualifications
- **Quality Assurance**: Document process certifications
- **Audit Trails**: Immutable compliance records

### 5.3 Token Economics

- **Learning Incentives**: Reward educational achievements
- **Verification Rewards**: Compensate certificate validators
- **Network Participation**: Incentivize mining and maintenance
- **Quality Assurance**: Economic incentives for standards compliance

---

## 5b. Quantum readiness

**The problem.** secp256k1 falls to Shor's algorithm, and Hikmalayer's exposure
is worse than Bitcoin's: a public key is published with *every* transaction,
and a validator's key sits in the on-chain staker set for as long as its stake
does. "Harvest now, decrypt later" applies today.

**The answer.** Two account types on one chain:

```
hkm…  classical      address = SHA256(secp_pub)[..20]              → ECDSA
hkq…  quantum-ready  address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
                                                                   → ECDSA AND ML-DSA-65
```

Both signatures are required, so the account is safe while **either** scheme
holds. The address commits to **both** public keys — without that, an attacker
who broke secp256k1 could pair the victim's classical key with an ML-DSA key of
their own, and the "hybrid" account would fall to a single break.

Enforced on transfers, HTS tokens, the AMM, staking, **unbonding**, and **block
production** — the last two matter most, because a validator's key is the
longest-lived key on the chain. The address decides which scheme applies, never
the transaction, so a hybrid account cannot be downgraded by omitting the
post-quantum half. `GENESIS_REQUIRE_HYBRID=1` makes an entire network
quantum-ready-only, committed to by the genesis state root.

**Cost:** ~5.4 KB per transaction against ~130 bytes, and ~11 ms to sign in a
browser. That is the real price of post-quantum signatures today, and it is why
hybrid is opt-in per account.

**Not covered:** the sr25519 VRF used for leader election is still classical. A
quantum adversary could *predict* a validator's slots — not forge blocks or
spend, since block signatures are separately hybrid. No standardized
post-quantum VRF exists yet.

Full detail: `docs/quantum_readiness.md`.

---

## 6. Security and Performance

### 6.1 Security

**Authorization.** Value-bearing calls are signature-authorized, not
session-authorized: the node rebuilds each canonical message from the request
fields and verifies against it, so a request the signature does not cover is not
a request. Signatures are scoped to a **chain id**, so one made for a testnet is
inert on mainnet. Each operation has its own message prefix, so a transfer
signature cannot authorize a stake. Nonces are strictly sequential per account.
Public keys have exactly one canonical encoding, so a transaction has one valid
on-wire form and one id.

**Consensus.** Fork choice is validator-progress first: **hashrate without stake
produces nothing and reorganizes nothing.** Finalized blocks are irreversible.
Equivocation is permissionlessly provable and burns stake on chain, and withdrawn
stake stays slashable throughout unbonding.

**Execution.** Checked arithmetic throughout with overflow checks on in release;
malformed recipients rejected rather than credited; authorization verified where
state changes rather than where a caller remembers to ask.

**Operations.** Admin and P2P endpoints are deny-by-default — an unset token
*disables* them. Tokens rotate without downtime and compare in constant time.
Gossip envelopes are signed and bound to a node id; peer reputation auto-bans
offenders. The node never accepts a private key.

**Stated limits.** No independent audit has been performed. Per-IP rate limiting
is not implemented — deploy behind a proxy. Admin tokens are bearer credentials,
not signatures. Transaction ordering within a block is chosen by its producer,
as on every chain, which is why AMM slippage bounds are mandatory in practice.

Detail: `docs/security_assessment.md` (13 findings, all fixed, each with a
regression test) and `docs/threat_model.md`.

### 6.2 Performance

**Measured** — Docker Compose multi-node, single host, 600-second sustained run:
8,940 transactions, 14.88 TPS average, ~67 ms latency, 0 reorgs, ~4–5 MB per
node.

**Scope, honestly.** That measures execution and API handling on one host. It is
**not** a wide-area consensus benchmark — propagation across real network paths,
fork resolution under partition and gossip at scale are unmeasured, and need a
public testnet with independent operators.

**Cryptographic costs**, which shape block size more than CPU does:

| | Classical | Quantum-ready |
|---|---|---|
| Signing | <1 ms | ~11 ms |
| Bytes per transaction | ~130 | ~5,400 |

**Storage** is persistent, written atomically and rebuilt by replaying blocks on
startup — rejected if replay fails rather than trusted. Checkpoint fast-sync
exists as an opt-in weak-subjectivity assumption; full replay is the default.

**Bounds by design:** mempool 1,000 transactions, 100 per block, 1 MiB bodies,
difficulty clamped 1–5, 15-second block target retargeted every 10 blocks.
Throughput is bounded by those parameters rather than by execution speed.

Detail: `docs/benchmark_report.md`.

---

## 7. Roadmap

Only genuinely undone work is listed. Anything already implemented is described
above and is not repeated here as a plan.

### 7.1 Launch blockers

1. **Independent external security audit.** Consensus, cryptography, state
   machine, P2P and node operations. Cannot be self-performed, and **post-quantum
   expertise must be in scope** — the dual-hybrid scheme is the newest code in
   the system. Guide: `docs/external_audit_guide.md`.
2. **Public adversarial testnet** with independent validators and real network
   conditions — the only setting where wide-area consensus behaviour gets
   measured.
3. **Genesis distribution policy**, published as on-chain vesting schedules so
   it is verifiable rather than promised.
4. **Production key management** — HSM or remote signer. Real constraint: most
   cannot produce ML-DSA-65 signatures today, so a hybrid validator's key is a
   software key.

### 7.2 Known gaps with no current answer

- **Post-quantum leader election.** The sr25519 VRF is classical; no
  standardized post-quantum VRF exists. The largest open cryptographic item.
- **No seed phrase / HD derivation.** Each account key needs its own backup.
- **Opening the validator set.** Removing the genesis allowlist should follow
  the adversarial testnet, not precede it.

### 7.3 Under consideration, not committed

Threshold or multi-signature accounts; raising throughput bounds against
public-testnet measurements; credential schema conventions.

### 7.4 Explicitly not pursued

- **A cross-chain bridge.** Hikmalayer will not custody external assets.
  Bridges are the industry's most-attacked component (Ronin $600M, Wormhole
  $320M, Nomad $190M). The cost of declining one is stated honestly in
  `docs/hts_and_listings.md`: a centralized listing requires bespoke
  integration. Reasoning: `docs/bridge_design.md`.
- **A general virtual machine.** §2.3 explains the trade.

---

## 8. Technical Specifications

### 8.1 Requirements

- **Node**: Linux/macOS/Windows, 2+ CPU cores, 512 MB+ RAM (2 GB+ for
  production), open HTTP port
- **Build**: Rust 1.75+, Cargo. Node.js 20+ for the SDK, dashboard and extension
- **Client**: any HTTP client with JSON support

### 8.2 Cryptography

| Purpose | Algorithm | Standard |
|---|---|---|
| Hashing, addresses, Merkle roots, PoW | SHA-256 | FIPS 180-4 |
| Classical signatures | secp256k1 ECDSA, low-S normalized | SEC 1 / SEC 2 |
| Post-quantum signatures | ML-DSA-65 | **FIPS 204** (NIST category 3) |
| Leader-election randomness | sr25519 VRF (Ristretto255) | — |
| Wallet vault | AES-256-GCM | NIST SP 800-38D |
| Vault key derivation | PBKDF2-HMAC-SHA256, 310,000 iterations | NIST SP 800-132 |

Sizes: secp256k1 65-byte key / 64-byte signature; ML-DSA-65 1,952-byte key /
3,309-byte signature.

### 8.3 Key dependencies

Axum (HTTP), Tokio (async runtime), Serde (serialization), `secp256k1`,
`schnorrkel` (VRF), `fips204` (ML-DSA), `sha2`/`sha3`. Clients use `@noble/curves`,
`@noble/hashes` and `@noble/post-quantum`, chosen so the browser and the node
produce byte-identical signatures — asserted by test.

### 8.4 Verification

139 Rust unit tests · 40 adversarial tests · 58 SDK offline tests including
Rust↔JS byte parity · 21 live integration tests · a 16-check end-to-end
application · `cargo clippy --all-targets -- -D warnings` clean · OpenAPI 3.1
spec lints clean.

---

## 9. Governance and Compliance

### 9.1 Development model

Development is led by Bestower Labs Limited. Protocol changes are published in
the repository and adopted by operators choosing to run the software — a
centralized development model, described as one.

**There is no on-chain governance.** No proposal system, no token-weighted
voting, no treasury contract. **HKM is not a governance token** and confers no
voting right; no mechanism exists in the protocol by which it could. With no
on-chain upgrade mechanism, a consensus change is a hard fork by definition.

Opening the validator set is the first meaningful decentralization step, and it
should follow the adversarial testnet.

### 9.2 Data protection

**No personal data goes on chain.** Proof-of-Credential publishes a *hash* of a
document, never the document. A hash is not the document, and it does not become
one by being on a blockchain.

On erasure, plainly: a blockchain cannot delete history, and any claim otherwise
is false. There is nothing on chain to delete — the document lives with the
issuer or subject under whatever obligations apply there, and **revocation is a
first-class on-chain operation**, which is the operative remedy. Subject
identifiers should be pseudonymous.

Addresses are pseudonymous, not anonymous: a chain is a permanent public record
and analysis can link addresses to identities.

Deployers remain responsible for lawful basis, consent, retention and transfer
of everything they hold off chain. Nothing here is legal advice.

**Not claimed:** ISO or W3C certification, VC/DID format conformance,
zero-knowledge proofs, or any third-party attestation.

---

## 10. Conclusion

Hikmalayer is a Layer 1 built for durability rather than breadth: verifiable
credentials and native assets, with cryptography chosen to still be standing
when secp256k1 is not.

**What distinguishes it:**

- **Dual-hybrid signatures enforced on every path a key authorizes** — including
  unbonding and block production, not only transfers
- **Hybrid consensus where hashrate without stake is worthless**
- **Capabilities as consensus objects** — no VM, so no contract for a bug to
  hide in, and no contract to audit before trusting a token
- **Credentials that publish a hash, not a document**

**What it does not claim:** independent review (none yet), quantum-safe leader
election (the VRF is classical), decentralization today (a launch allowlist may
gate validators), or exchange listings (no bridge means bespoke integration).

Every trade-off above has a cost, and the costs were chosen deliberately and
written down so they can be argued with. The protocol is built and tested; what
remains before mainnet is external validation rather than missing code. That is
a good position, and it is not the same as being finished.

---

## References

**Cryptography:**

- **NIST FIPS 204 (2024): Module-Lattice-Based Digital Signature Standard
  (ML-DSA)** — the post-quantum scheme implemented here
- NIST FIPS 180-4: Secure Hash Standard
- Shor, P.W. (1994). "Algorithms for Quantum Computation: Discrete Logarithms
  and Factoring" — why secp256k1 is not durable
- Grover, L.K. (1996). "A Fast Quantum Mechanical Algorithm for Database Search"
  — why SHA-256 is
- Bindel, N. et al. (2017). "Transitioning to a Quantum-Resistant Public Key
  Infrastructure" — hybrid signature combiners

**Consensus:**

- Nakamoto, S. (2008). "Bitcoin: A Peer-to-Peer Electronic Cash System"
- David, B., Gaži, P., Kiayias, A., Russell, A. (2018). "Ouroboros Praos" —
  the VRF leader-election and withhold-bias model
- Micali, S., Rabin, M., Vadhan, S. (1999). "Verifiable Random Functions"

**Implementation:**

- Rust: https://doc.rust-lang.org/ · Axum: https://docs.rs/axum/
- `fips204` (ML-DSA), `schnorrkel` (sr25519 VRF), `@noble/post-quantum`

**Referenced but not conformed to** (see the full whitepaper §12.3):

- W3C Verifiable Credentials: https://www.w3.org/TR/vc-data-model/ — a VC can be
  anchored here by hash; the format is not implemented
- GDPR (EU 2016/679), FERPA — obligations fall on deployers handling documents
  off chain, not on the protocol, which stores no personal data

---

_Released under Creative Commons Attribution 4.0 International (CC BY 4.0).
Informational only: nothing here is an offer, a solicitation, investment advice
or legal advice. The full whitepaper is `docs/Whitepaper.md`._
