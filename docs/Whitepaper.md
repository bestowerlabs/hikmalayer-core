# Hikmalayer: A Quantum Dual-Hybrid Layer 1 for Digital Credentials and Native Assets

**Official Whitepaper**  
**Version 2.0**  
**August 2026**  
**License:** Creative Commons Attribution 4.0 International (CC BY 4.0)

**Author:**  
Muhammad Ayan Rao  
Founder & Director,  
Bestower Labs Limited  
Ayanrao@bestowerlabs.com  
www.bestowerlabs.com



---

## Abstract

Hikmalayer is a sovereign Layer 1 blockchain, written in Rust, that is **dual-hybrid
in two independent senses**:

1. **Hybrid consensus.** Stake-weighted VRF selection chooses each block's
   leader (Proof of Stake); that leader then finalizes the block with
   Proof of Work. Neither stake alone nor hashpower alone produces a block.
2. **Hybrid cryptography.** An account may be *quantum-ready*: authorized by
   **two** signatures over the same message — secp256k1 ECDSA **and** ML-DSA-65
   (FIPS 204, NIST post-quantum category 3). Forging one transaction requires
   breaking **both** schemes, so the account remains secure while *either*
   holds.

The token layer and the exchange layer are **protocol-native**. HKM is the
native coin; HTS (Hikmalayer Token Standard) assets and a constant-product
automated market maker are consensus objects executed by the state machine
itself — not smart contracts, because Hikmalayer deliberately ships **no
virtual machine**. There is **no bridge**, and none is planned: no wrapped or
external asset is or will be tradeable on this chain.

Its flagship application is Proof-of-Credential: verifiable credentials as
first-class consensus objects, where only a document *hash* is ever published,
and any third party can verify a credential against the block-committed state
root without trusting a node.

This whitepaper is released under CC BY 4.0 to encourage broad distribution,
translation, and academic citation while preserving attribution to Bestower
Labs Limited and the Hikmalayer team. It describes the system **as
implemented**; where something is planned rather than built, it says so.

**Keywords:** Blockchain, Layer 1, Post-Quantum Cryptography, ML-DSA, FIPS 204,
Hybrid Signatures, Proof-of-Stake, Proof-of-Work, VRF, Verifiable Credentials,
Native Token Standard, Automated Market Maker

---

## Reader's summary — what Hikmalayer is, in one page

| | |
|---|---|
| **Type** | Sovereign Layer 1. Own address format, own signing domain, no dependency on Ethereum or any other chain |
| **Consensus** | PoS leader selection (stake-weighted, VRF-seeded) + PoW finalization by the selected leader |
| **Randomness** | sr25519 VRF beacon — validator-unique per slot, nothing to grind |
| **Native coin** | HKM — pays fees, secures the chain through staking, and is the unit block rewards are paid in |
| **Native token standard** | HTS — fungible assets as consensus objects, fixed supply at creation, reducible only by burning |
| **Native exchange** | Constant-product AMM (HKM ↔ HTS) in the state machine, 0.30% fee to liquidity providers |
| **Smart contracts** | **None.** No VM, no user-deployed bytecode. Capabilities are protocol features (see §4) |
| **Bridge** | **None, by decision.** No wrapped or external asset exists on this chain |
| **Account types** | `hkm…` classical (ECDSA) and `hkq…` quantum-ready (ECDSA **+** ML-DSA-65) |
| **Post-quantum** | Opt-in per account; a network may require it at genesis. Covers transfers, tokens, the AMM, staking, unbonding and block production |
| **Not post-quantum** | The sr25519 VRF used for leader election — stated plainly in §7.1.5 |
| **Flagship application** | Proof-of-Credential — hash-only on-chain credentials, verifiable against the state root |

---

## 1. Introduction

### 1.1 Background and Motivation

The digital transformation of modern organizations has created an unprecedented demand for secure, verifiable, and tamper-proof systems for managing credentials, assets, and transactions. Traditional centralized systems suffer from single points of failure, limited transparency, and vulnerability to fraud. While existing blockchain platforms provide solutions to these challenges, they often lack the specialized features required for certificate management and struggle with complexity, scalability, or integration challenges.

Hikmalayer emerges as a purpose-built solution that bridges these gaps by offering a streamlined yet powerful blockchain platform specifically optimized for digital certificate management and token-based economies. The platform recognizes that modern organizations require not just a blockchain, but a complete ecosystem that can handle diverse use cases ranging from academic credentials to professional certifications and asset tokenization.

Hikmalayer is stewarded by Bestower Labs Limited, which provides the operational and governance
framework for the protocol’s evolution.

### 1.2 Problem Statement

Contemporary digital credential systems face several critical challenges:

1. **Verification Complexity**: Manual verification processes are time-consuming, error-prone, and lack real-time validation capabilities
2. **Trust Dependencies**: Centralized credential authorities create single points of failure and trust bottlenecks
3. **Interoperability Issues**: Isolated credential systems prevent cross-platform verification and recognition
4. **Fraud Vulnerability**: Traditional certificates are susceptible to forgery, tampering, and unauthorized duplication
5. **Scalability Constraints**: Existing blockchain solutions often struggle with transaction throughput and operational efficiency
6. **Integration Barriers**: Complex implementation requirements limit adoption by organizations with varying technical capabilities

### 1.3 Solution Overview

Hikmalayer addresses these challenges through a comprehensive blockchain platform that combines:

- **Hybrid PoS/PoW consensus** — stake-weighted VRF selection picks the leader;
  that leader finalizes the block with Proof of Work. Hashrate without stake
  produces nothing.
- **Hybrid post-quantum cryptography** — an account may require *two*
  signatures, ECDSA and ML-DSA-65, so forging one transaction means breaking
  both schemes.
- **Proof-of-Credential** — credentials as consensus objects, where only a
  document *hash* is published and verification runs against the block-committed
  state root rather than against a node's word.
- **Protocol-native tokens and exchange** — HTS assets and a constant-product
  AMM executed by the state machine. **No virtual machine**, so there is no
  contract for a bug to hide in, and no contract to audit before trusting a
  token.
- **A native signing domain** — own address format, own message prefixes,
  network-scoped signatures. No dependency on another chain's conventions, and
  no cross-network replay.
- **Deny-by-default operations** — admin and P2P endpoints are disabled unless
  their tokens are set, and the node never accepts a private key.

---

## 2. Technical Architecture

### 2.1 System Overview

Hikmalayer is written entirely in Rust, for its memory safety and for the fact
that a consensus bug caused by a use-after-free is a class of failure this
design simply does not have.

```
┌─────────────────────────────────────────────────────────────┐
│  Clients                                                     │
│  REST API · @hikmalayer/sdk · browser wallet · MV3 extension │
│  · hikma-wallet CLI (offline)                                │
├─────────────────────────────────────────────────────────────┤
│  Protocol-native capabilities            (NO virtual machine)│
│  HTS tokens · AMM · vesting · credentials · staking          │
├─────────────────────────────────────────────────────────────┤
│  Authorization                                               │
│  secp256k1 ECDSA  ·  ML-DSA-65 for hkq accounts              │
│  chain-id scoping · per-operation domains · canonical encoding│
├─────────────────────────────────────────────────────────────┤
│  Consensus                                                   │
│  PoS leader selection · sr25519 VRF beacon · PoW finalization │
│  sovereign finality · slashing                               │
├─────────────────────────────────────────────────────────────┤
│  Replicated state machine                                    │
│  balances · nonces · stakers · tokens · pools · credentials   │
│  committed by a per-block STATE ROOT                         │
├─────────────────────────────────────────────────────────────┤
│  Storage & networking                                        │
│  atomic persistence · replay from genesis · checkpoint sync   │
│  signed P2P envelopes · peer scoring                         │
└─────────────────────────────────────────────────────────────┘
```

The load-bearing idea is the **state root**. Every block commits to the full
chain state *after* executing it, so no node can forge a balance, a credential
or a validator set without every other node detecting it by re-execution. Trust
is replaced by arithmetic at every layer above.

### 2.2 Core Components

#### 2.2.1 Blocks

```rust
pub struct Block {
    pub index: u64,
    pub timestamp: DateTime<Utc>,
    pub transactions: Vec<String>,
    pub merkle_root: String,                     // commits to the exact contents
    pub state_root: String,                      // commits to state AFTER execution
    pub previous_hash: String,
    pub difficulty: usize,
    pub nonce: u64,                              // Proof-of-Work solution
    pub hash: String,
    pub validator: Option<String>,
    pub validator_public_key: Option<String>,
    pub validator_signature: Option<String>,     // ECDSA over the block hash
    pub validator_pq_signature: Option<String>,  // ML-DSA-65, for hkq validators
    pub vrf_output: Option<String>,              // randomness beacon contribution
    pub vrf_proof: Option<String>,
}
```

The mined hash covers the index, Merkle root, **state root**, timestamp,
validator identity and previous hash — so Proof-of-Work is spent on a commitment
to the execution result, not merely to a list of transactions.

#### 2.2.2 Transaction types

Every capability is a transaction type; there is no bytecode (§4).

| Group | Types |
|---|---|
| Value | `Transfer`, `Vest` |
| Consensus | `Stake`, `Withdraw`, `Slash`, `Reward` |
| Tokens (HTS) | `TokenCreate`, `TokenTransfer`, `TokenBurn` |
| Exchange (AMM) | `AddLiquidity`, `RemoveLiquidity`, `Swap` |
| Credentials | `Certificate` (issue / revoke) |

Each carries its sender, a strictly sequential per-account nonce, the network's
chain id, and the signature(s) the sender's **address type** requires.

#### 2.2.3 Authorization

Verification happens where state changes, not where a caller remembers to ask:
`apply_transaction` verifies the sender's signature itself, so a code path that
forgets to check cannot admit a forged transaction. See §7.1 for the full
cryptographic architecture.

#### 2.2.4 Consensus

Stake-weighted VRF selection chooses the leader; that leader mines the block.
Full detail in §3.

#### 2.2.5 Storage and networking

State is persistent and written atomically (temp file plus rename), rebuilt by
replaying blocks on startup and **rejected if replay fails** rather than trusted.
Gossip envelopes are signed by the sender's node key and bound to its derived
node id. Checkpoint fast-sync exists as an explicit, opt-in weak-subjectivity
assumption; full replay remains the default.

### 2.3 Token and exchange layer

**HKM** is the native coin: it pays every fee, secures the chain through
staking, and is what block rewards are paid in. It is created by consensus and
by nothing else — there is no mint operation, only the emission schedule (§5.1).

**HTS** assets are consensus objects with a **fixed supply at creation**;
supply moves only downward, through burning. The **AMM** pairs HKM against any
HTS token with a constant-product invariant. Both are detailed in §4.4–4.5 and
§5.

### 2.4 API architecture

A REST API over JSON, with an OpenAPI 3.1 description in `docs/openapi.yaml`.

| Category | What it does |
|---|---|
| Blockchain | blocks, chain statistics, state root, validation |
| Accounts | balances, nonces, vesting schedules |
| Value | transfers, staking, unbonding, vesting |
| Tokens (HTS) | registry, issuance, transfer, burn, balances |
| DEX | pools, positions, read-only quotes, swap, add/remove liquidity |
| Credentials | issue, revoke, verify (returns a state-root-bound proof) |
| Validators | validator set, block proposal and submission |
| P2P | signed protocol envelopes, chain sync, checkpoint bundles |
| Admin | faucet, mining trigger, difficulty, governance — token-gated |

Two rules shape the whole surface:

1. **Value-bearing calls are signature-authorized, not session-authorized.**
   There is no login. The node rebuilds each canonical message from the request
   fields and verifies against it, so a request the signature does not cover is
   not a request.
2. **Administrative endpoints are deny-by-default.** An unset token *disables*
   the endpoint rather than opening it.

Submission is not execution: a transaction is signature-checked and **queued**,
and changes state only when mined into a block. Reads reflect on-chain state.

## 3. Consensus Mechanism

### 3.1 Hybrid PoS → PoW

**Stake decides who may produce a block; work decides that it was produced.**
Neither alone is sufficient.

**1 — Leader selection (Proof of Stake).** A stake-weighted draw over the
on-chain validator set **as of the parent state**, seeded by the VRF randomness
beacon. The slot input is salted with the block height, so one parent hash can
never be reused to claim a different slot.

**2 — Liveness rotation.** Round 0's leader is the primary. Each elapsed
30-second slot timeout opens the next round's leader as a fallback, so an
offline validator delays the chain by at most one timeout rather than stalling
it. A block must come from the **smallest open round** that selects its
producer, and its VRF must verify against exactly that round's slot input.

**3 — Finalization (Proof of Work).** The selected leader — and only that
leader — mines the block. Difficulty is derived deterministically by the
retargeting schedule (15-second target, retargeted every 10 blocks) and clamped
to 1–5 hex zeros, so a malformed value can neither disable Proof-of-Work nor
stall a node. There is no external miner and none is needed.

**4 — Signing.** The leader signs the block hash with its registered key. A
`hkq…` validator signs under **both** schemes, and the key registered on chain —
not anything in the block — decides whether the post-quantum half is required.

**5 — Validation.** Every node independently re-checks: the producer is selected
for the smallest open round at the parent state; the VRF proof verifies against
that round's slot input and the registered VRF key; the Proof-of-Work meets the
consensus-derived difficulty; the signer's key **equals** the one registered on
chain; both signatures verify where applicable; timestamps are bounded in both
directions; the Merkle root matches the payload; and the **state root matches
re-execution**.

### 3.2 Randomness

Every block carries an sr25519 VRF proof over its slot input. A VRF output is
unique for a given (key, input), so a validator cannot search for a value that
improves its own odds — there is nothing to grind. Outputs fold into an on-chain
beacon seeding subsequent selection.

**Residual bias, stated:** a selected leader may *withhold* its block —
forfeiting the reward — to avoid contributing its randomness. This is the
standard Praos/RANDAO bound and applies to every VRF-based chain; it is bounded
and costly, not eliminated.

### 3.3 Fork choice and finality

**Validator-progress first.** Finalized blocks are irreversible. A fork must
carry *more validator-sealed blocks* to displace the local chain; cumulative
Proof-of-Work only breaks exact ties. Fork tips future-dated beyond the
clock-skew bound are rejected outright.

The consequence is worth stating plainly: **hashrate without stake produces
nothing and reorganizes nothing.** This is a different security model from pure
Proof-of-Work, and the reason "51% of hashpower" is not the relevant threat here.

An adopted chain is **re-executed under local network parameters** and its state
rebuilt from genesis. A candidate's claims about its own genesis are never
trusted — which is what stops a peer presenting a plausible-looking chain from a
different network.

### 3.4 Accountability

Staking and unbonding are signed on-chain transactions, so the validator set is
derived from state rather than node-local bookkeeping.

- **Slashable:** equivocation (two blocks for one slot), a bad signature,
  invalid Proof-of-Work, a tampered payload or state root, or producing outside
  the open rounds. Proofs are permissionless and burn stake on chain;
  double-slashing for one offence is prevented.
- **Not slashable:** being offline or slow.
- **Unbonding:** withdrawn stake stays locked and slashable for the unbonding
  period, and the slashing window equals it, so misbehaving stake can never exit
  ahead of its punishment. A withdrawal must either exit fully or leave at least
  the validator minimum.

### 3.5 Block economics

Rewards are **implemented and consensus-verified per height**, not planned: a
block claiming a reward that does not match the schedule for its height is
invalid. The initial reward is 3,700 HKM, halving every 9,500,000 blocks, with a
perpetual 50 HKM tail emission as the long-run security budget. Transaction fees
follow a dynamic base fee that lives in the state root and is recomputed
identically by every node, and are paid to the block's validator. Full
parameters in §5.

---

## 4. Protocol-Native Capabilities (and why there is no VM)

### 4.1 The design decision, stated plainly

**Hikmalayer has no virtual machine.** There is no user-deployable bytecode,
no Solidity, no WASM runtime, and no `eth_call` equivalent. Everything the
chain can do is a transaction type the state machine executes directly.

This is a deliberate trade, and it costs something real: you cannot write an
arbitrary application that runs on Hikmalayer the way you can on Ethereum.
What it buys:

- **The attack surface is enumerable.** A general VM means every deployed
  contract is new consensus-adjacent code written by someone with no
  obligation to be careful, and the majority of value ever stolen from
  blockchains has been stolen through contract bugs rather than consensus
  ones. Hikmalayer's surface is the transaction types in this document.
- **No gas metering, and no gas-metering bugs.** Fees are per-transaction and
  deterministic, because execution cost is bounded by construction.
- **Upgrades are protocol changes.** Adding a capability means changing the
  chain, in the open, with the state root as the arbiter — not deploying an
  unaudited contract that a user cannot distinguish from a safe one.

An earlier version of this document described a "smart contract framework".
That was inaccurate: the component in question (`ContractExecutor`) is a
credential registry, not an execution environment. This section replaces it.

### 4.2 What the protocol provides instead

| Capability | Consensus objects | Transaction types |
|---|---|---|
| Native value | account balances, nonces | `Transfer` |
| Staking | `StakeInfo` (stake, secp256k1 key, VRF key, ML-DSA key) | `Stake`, `Withdraw`, `Slash` |
| Token standard (HTS) | `TokenInfo`, per-token balances | `TokenCreate`, `TokenTransfer`, `TokenBurn` |
| Exchange (AMM) | `AmmPool` (reserves, LP shares) | `AddLiquidity`, `RemoveLiquidity`, `Swap` |
| Vesting | vesting schedules | `Vest` |
| Credentials | credential records | `Certificate` (issue / revoke) |
| Block economics | fee pot, emission schedule | `Reward` |

Each is executed by `ChainState::apply`, committed to by the state root, and
re-verified by every node. None of them can be redefined by a user.

### 4.3 Proof-of-Credential

The flagship capability, and the reason the chain exists.

**Only a hash goes on chain.** A credential record holds an issuer, a subject,
a document hash, a revocation flag and a height. The document itself never
touches the chain, so publishing a credential does not publish its contents.

**Issuance and revocation are both signed transactions** authorized by the
issuer's key — and revocation is a first-class operation, not a convention.
An issuer can withdraw a credential it granted, instantly and verifiably; a
credential that cannot be revoked is not a credential, it is a receipt.

**Verification needs no trusted node.** A verifier receives the credential,
the height, the state root and the block hash, and checks the record against
the state root committed in that block. A node that lies about a credential is
caught by arithmetic, not by reputation.

```bash
# Issue: only the DOCUMENT HASH is published
hikma-wallet sign-credential <id> <subject> <sha256-of-document> false <nonce> <key>

# Verify: returns { credential, height, state_root, block_hash }
curl -s "$NODE/credentials/verify/<id>"
```

### 4.4 HTS — the native token standard

An HTS token is a consensus object, not a contract:

- **Deterministic id.** `hkt` + hex(SHA-256(`creator:symbol:nonce`)[..20]).
- **Fixed supply at creation.** There is no mint operation. Supply moves in
  one direction only, downward, through `TokenBurn`. No issuer — including the
  original creator — can inflate a token after the fact.
- **Up to 18 decimals**, declared at creation and immutable.
- **The same authorization rules as HKM**, including the hybrid rules: a
  `hkq…` issuer's token operations need both signatures.

Because there is no contract, there is no such thing as an HTS token with a
hidden mint function, a blacklist, a transfer hook, a proxy admin, or an
upgradeable implementation. The properties above are the properties of *every*
HTS token, guaranteed by consensus rather than by an audit of that particular
token's code.

The honest trade-off is the same one as §4.1: there is also no way to build an
HTS token with legitimate custom behaviour — no rebasing, no fee-on-transfer,
no programmable vesting beyond the protocol's own `Vest`.

### 4.5 The native AMM

A constant-product (`x·y = k`) automated market maker, in the state machine.

- **Pairs are HKM ↔ HTS token.** Every pool has HKM on one side.
- **0.30% fee**, retained in the reserves and therefore accruing to liquidity
  providers pro rata.
- **`MINIMUM_LIQUIDITY` (1,000 shares) is locked permanently** on a pool's
  first deposit, which keeps `total_shares` above zero forever and closes the
  first-depositor share-inflation attack.
- **Slippage bounds are mandatory in practice.** `Swap` carries `min_out`;
  `AddLiquidity` carries `min_shares`; `RemoveLiquidity` carries `min_hkm` and
  `min_token`. The SDK will not build an unbounded trade even if asked, since
  an unbounded swap is a gift to whoever orders the block.

Because the AMM is consensus code, a trade cannot be front-run *by a contract*,
there is no router to be tricked into approving, and there is no pool
implementation to differ from another pool's. Ordering within a block is still
chosen by the block's producer — that is true of every blockchain, and §7.3
says what does and does not follow from it.

---

## 5. Token Economics

### 5.1 HKM — the native coin

HKM is not a token issued on Hikmalayer; it is the chain's own coin, created by
consensus and by nothing else.

| Parameter | Value |
|---|---|
| Decimals | 6 — 1 HKM = 1,000,000 base units |
| Total supply | ~100 billion HKM at tail start |
| Genesis allocation | 30 billion HKM (30%) |
| Mined | ~70 billion HKM (70%) |
| Initial block reward | 3,700 HKM |
| Halving interval | 9,500,000 blocks (~4.5 years at 15-second targets) |
| Tail emission | 50 HKM per block, perpetual |
| Validator minimum stake | 10,000 HKM |
| Unbonding period | 20 blocks, slashable throughout |

**Why a tail emission.** A schedule that halves to zero eventually pays
validators nothing but fees, and a chain whose security budget depends entirely
on fee volume is fragile precisely when volume falls. The 50 HKM tail is a
permanent, predictable security budget. It is inflationary in absolute terms and
asymptotically zero in relative terms, and that trade is stated rather than
hidden.

**Emission is consensus-verified per height.** A block claiming a reward that
does not match the schedule for its height is invalid. No operator can pay
themselves more by patching a node.

### 5.2 Fees

A dynamic base fee, EIP-1559 in shape, with a floor of 0.001 HKM and a cap of
100 HKM. It is recomputed deterministically each block from the parent's
congestion, lives in the state root, and is therefore identical on every node.
The fee is paid to the block's validator.

Fees are always in HKM, including for HTS token operations and AMM trades.
That is deliberate: it means every user of the chain, whatever they are
actually transacting in, has a reason to hold the asset that secures it.

### 5.3 HKM's three roles

1. **Settlement and fees.** Every transaction pays in HKM.
2. **Security.** Stake is denominated in HKM; the validator set is
   stake-weighted; slashing burns HKM.
3. **The AMM's numeraire.** Every liquidity pool pairs HKM against an HTS
   token, so HKM is the settlement asset of the chain's own economy.

### 5.4 On-chain vesting

Allocations are locked by protocol, not by promise. A `Vest` transaction places
funds in a consensus-managed pool that releases block by block after a cliff.
Schedules are inspectable by anyone at `GET /vesting/{address}`.

The distinction matters for a genesis distribution: a team allocation published
as an on-chain vesting schedule is a constraint the chain enforces, and a claim
in a blog post is not.

### 5.5 HTS tokens are a different thing entirely

HTS assets (§4.4) are issued *on* the chain by anyone, permissionlessly. They:

- have a **fixed supply at creation** — there is no mint operation, and supply
  moves only downward through burning;
- pay their fees in HKM;
- do not stake, do not secure the chain, and earn no block rewards.

Because there is no contract, no HTS token can have a hidden mint function, a
blacklist, a transfer hook, a proxy admin or an upgradeable implementation.
Those properties hold for *every* HTS token by consensus rather than by an audit
of that particular token's code. The same absence means an HTS token cannot have
legitimate custom behaviour either — no rebasing, no fee-on-transfer, no
programmable vesting beyond the protocol's own.

### 5.6 Distribution, and what is not yet decided

The genesis parameters above are implemented and consensus-verified. **The
allocation policy for the 30 billion genesis treasury is a business decision and
is not settled in this document.** Any real launch should publish it as on-chain
vesting schedules at genesis, where it can be verified rather than trusted.

Stating this openly is more useful than a placeholder pie chart.

---

## 6. Use Cases and Applications

### 6.1 Academic Credentials

**Digital Diploma Management:**
Educational institutions can leverage Hikmalayer to issue tamper-proof digital diplomas and certificates:

- **Issuance Process**: Universities create certificates on-chain with student information, degree details, and institutional signatures
- **Verification System**: Employers and third parties can instantly verify academic credentials without contacting the issuing institution
- **Fraud Prevention**: Blockchain immutability prevents diploma mills and credential forgery
- **International Recognition**: Cross-border credential verification without complex bureaucratic processes

**Professional Certifications:**
The platform supports professional certification bodies in managing industry credentials:

- **Continuing Education**: Track and verify ongoing professional development requirements
- **License Management**: Maintain professional licenses with automatic expiration and renewal tracking
- **Competency Verification**: Demonstrate specific skills and qualifications to potential employers
- **Industry Standards**: Ensure certifications meet recognized industry benchmarks

### 6.2 Corporate Training and Compliance

**Employee Certification Programs:**
Organizations can implement comprehensive training and certification tracking:

- **Skills Assessment**: Document employee competencies and skill development
- **Compliance Training**: Ensure regulatory training requirements are met and documented
- **Career Progression**: Track employee advancement and qualification achievements
- **Audit Compliance**: Provide immutable records for regulatory audits and inspections

**Supply Chain Verification:**
The certificate system extends to supply chain and quality assurance applications:

- **Product Certification**: Verify product quality, origin, and compliance standards
- **Vendor Qualification**: Document supplier certifications and quality metrics
- **Process Validation**: Ensure manufacturing processes meet specified standards
- **Traceability**: Maintain complete audit trails for critical supply chain components

### 6.3 Digital Identity and KYC

**Identity Verification:**
Hikmalayer can serve as a foundation for decentralized identity management:

- **Identity Proofing**: Cryptographically verifiable identity credentials
- **Privacy Protection**: Selective disclosure of identity attributes as needed
- **Cross-Platform Recognition**: Portable identity credentials across different services
- **Reduced Verification Overhead**: Streamlined KYC processes for financial services

**Professional Reputation:**
Build and maintain professional reputation through verifiable achievements:

- **Skill Endorsements**: Peer-verified professional capabilities
- **Project Credentials**: Documented contributions to successful projects
- **Performance Metrics**: Quantifiable professional achievements and outcomes
- **Reference Verification**: Authenticated professional references and recommendations

### 6.4 Tokenized Incentive Systems

**Learning Rewards:**
Educational platforms can implement token-based incentive systems:

- **Achievement Rewards**: Tokens for completing courses, certifications, or milestones
- **Peer Recognition**: Community-driven reward systems for helpful contributions
- **Knowledge Sharing**: Incentives for creating educational content and resources
- **Continuous Learning**: Long-term rewards for ongoing skill development

**Community Participation:**
Foster active community engagement through token incentives:

- **Content Contribution**: Rewards for high-quality educational materials
- **Mentorship Programs**: Token incentives for experienced professionals mentoring newcomers
- **Quality Assurance**: Rewards for verifying and validating community-generated content
- **Network Effects**: Growing value as network participation increases

### 6.5 Integration Scenarios

**Enterprise Integration:**
Hikmalayer's API-first approach enables seamless integration with existing enterprise systems:

- **HR Systems**: Integration with human resources platforms for employee certification tracking
- **Learning Management**: Connection with LMS platforms for automated certificate issuance
- **ERP Integration**: Supply chain and quality management system integration
- **Third-Party Services**: API connections with verification services and background check providers

**Cross-Platform Interoperability:**
The platform supports integration with other blockchain networks and traditional systems:

- **Bridge Protocols**: Future support for cross-chain certificate recognition
- **Legacy System Integration**: APIs for connecting with existing credentialing systems
- **Standard Compliance**: Adherence to emerging standards for digital credentials
- **Migration Pathways**: Tools for migrating existing credential data to blockchain storage

---

## 7. Security Analysis

### 7.1 Cryptographic Architecture

This section describes what is implemented, not what is intended.

#### 7.1.1 Primitives in use

| Purpose | Primitive | Quantum status |
|---|---|---|
| Account signatures (classical) | secp256k1 ECDSA, low-S normalized | **Broken by Shor's algorithm** |
| Account signatures (quantum-ready) | secp256k1 ECDSA **+** ML-DSA-65 (FIPS 204) | Safe while *either* holds |
| Block signatures | as above, per the validator's account type | as above |
| Leader-election randomness | sr25519 VRF (Ristretto255, schnorrkel) | **Broken by Shor's algorithm** — see §7.1.5 |
| Hashing, addresses, Merkle roots, PoW | SHA-256 | Sound. Grover halves it; 128-bit effective is out of reach |
| Wallet storage | AES-256-GCM, PBKDF2-SHA256 (310,000 iterations) | Sound. 128-bit effective post-Grover |

#### 7.1.2 The native signing domain

Every account signature is over

```
SHA256( "\x19Hikmalayer Signed Message:\n" ‖ <UTF-8 byte length> ‖ <chain_id> ":" <canonical message> )
```

Three properties come from that construction:

- **No cross-chain reuse.** There is no `0x`/keccak address and no
  `personal_sign` prefix, so a Hikmalayer signature is meaningless to another
  chain's verifier and vice versa.
- **No cross-*network* replay.** The chain id is inside the signed bytes, and
  it is a *visible* prefix rather than a hidden digest tweak, so a wallet's
  confirmation screen can show which network is being authorized. A
  transaction signed on a testnet is inert on mainnet even though the account
  exists on both.
- **No cross-operation reuse.** Each operation has its own message prefix
  (`hikmalayer-transfer:`, `hikmalayer-stake:`, …), so a transfer signature
  cannot authorize a stake.

#### 7.1.3 The quantum problem, stated without euphemism

Hikmalayer's classical exposure is **worse than Bitcoin's**, and pretending
otherwise would be dishonest.

Bitcoin's P2PKH addresses publish only a *hash* of the public key; the key
appears when a coin is first spent, so a never-spent output is not exposed.
Hikmalayer publishes the public key with **every** transaction, because
verification takes it from the request. Worse, a validator's key sits in
`StakeInfo` in the open for as long as its stake does — there is no "spend once
and rotate" for it. The longest-lived, most valuable key on the chain is also
the most exposed.

"Harvest now, decrypt later" therefore applies directly: an adversary can
archive the chain today and derive private keys the day the hardware exists.

#### 7.1.4 The dual-hybrid answer

```
hkm…   classical      address = SHA256(secp_pub)[..20]
                      authorized by:  ECDSA

hkq…   quantum-ready  address = SHA256("hikmalayer-hybrid-address-v1"
                                        ‖ secp_pub ‖ mldsa_pub)[..20]
                      authorized by:  ECDSA  AND  ML-DSA-65
```

**Both signatures are required.** An attacker must break both schemes to forge
one transaction; the account is safe while either holds. This is why the answer
is hybrid rather than a replacement: nobody can honestly say when a
cryptographically relevant quantum computer arrives, and equally ML-DSA is
young enough that a classical break of *it* would not be shocking. Requiring
both means neither surprise is fatal.

**The address commits to both public keys.** This is the part that is easy to
get wrong, and getting it wrong makes the whole exercise decorative. If a
hybrid address were derived from the classical key alone, an attacker who broke
secp256k1 could present the victim's classical key alongside an ML-DSA key *of
their own*, and the "hybrid" account would fall to a single break. Hashing both
means substituting either names a different account.

**Parameter choice.** ML-DSA-65 (NIST category 3). ML-DSA-44 is category 2 — a
thinner margin than a chain holding value for decades should accept. ML-DSA-87
is category 5 and costs another ~1.3 KB per signature against an adversary
nobody can presently describe.

**Determinism.** Keys derive from FIPS 204's own 32-byte seed ξ, itself derived
with domain separation from the account's existing private key — so one backup
still covers both schemes. Signatures use the hedged variant with the
randomness pinned to a seed derived from (key, message): standards-conforming,
reproducible across implementations, and proof against a broken RNG on a user's
machine leaking a key through a signature. Determinism is also what lets the
Rust node and the browser wallet produce byte-identical signatures, which is
asserted by test rather than assumed.

**Where it is enforced.** Everywhere a key authorizes value or consensus:

| Path | Classical account | Quantum-ready account |
|---|---|---|
| Transfers, HTS tokens, vesting, AMM | ECDSA | ECDSA **+ ML-DSA-65** |
| Staking | ECDSA | Both; the ML-DSA key is registered on chain |
| Unbonding stake | ECDSA vs. the on-chain key | **Both**, vs. the on-chain keys |
| Block production | ECDSA over the block hash | **Both** over the block hash |

In every case **the address decides**, never the transaction — otherwise an
attacker could downgrade a hybrid account simply by omitting the post-quantum
half. Symmetrically, a classical transaction carrying post-quantum fields is
rejected rather than ignored, so one authorized transaction has exactly one
valid encoding.

**Network-level enforcement.** `GENESIS_REQUIRE_HYBRID=1` makes a network
accept only `hkq…` senders. The flag is committed to by the genesis state root,
so it is a property of the network and not a local setting nodes could quietly
differ on.

**The cost, honestly.**

| | Classical | Quantum-ready |
|---|---|---|
| Public key | 65 bytes | 2,017 bytes |
| Signature | 64 bytes | 3,373 bytes |
| Per transaction | ~130 bytes | ~5.4 KB (≈40×) |
| Signing time (browser) | <1 ms | ~11 ms |

This is the real, unavoidable price of post-quantum signatures today. It is why
hybrid is opt-in per account rather than mandatory on every network.

**Migration is a transfer, not an upgrade.** A key's classical and hybrid
accounts are *different accounts with separate balances*. There is deliberately
no mechanism by which a classical signature can claim a hybrid account's funds
— such a mechanism would be the downgrade attack, shipped as a feature.

#### 7.1.5 What is NOT quantum-protected

Stated here rather than buried:

- **The sr25519 VRF used for leader election remains classical.** A quantum
  adversary that recovered a validator's VRF key could **predict** that
  validator's future slots. It could not mint, spend, or produce a block —
  block signatures are separately hybrid — but slot prediction is a real
  advantage for a targeted denial-of-service or a grinding-adjacent strategy.
  There is no standardized post-quantum VRF today; the available options are a
  hash-based construction with weaker unpredictability guarantees, or waiting
  for standardization. Hikmalayer waits, and says so.
- **Classical accounts remain classical.** `hkm…` accounts carry the exposure
  described in §7.1.3. Hybrid is opt-in.
- **Nothing here defends against conventional key theft.** Malware, a phished
  backup, an unlocked wallet — hybrid changes none of it.

#### 7.1.6 Encoding canonicality

One authorized transaction has exactly **one** valid on-wire form. secp256k1
accepts a 33-byte compressed encoding of the same key and hex accepts upper
case; both were once accepted, both hashed to the same address, and both
verified the same signatures — which meant a relay could re-encode a
transaction in flight and produce a different, equally valid transaction id.
The chain now accepts only the canonical encoding (uncompressed, 65 bytes,
`04`-prefixed, lower-case hex; lower-case hex for ML-DSA keys), verified by
round-trip rather than by pattern match.

#### 7.1.7 Block and chain integrity

- **Merkle-root transaction commitment.** Implemented — a binary tree over the
  transaction payloads, with odd nodes paired against themselves.
- **State root.** Every block commits to the full chain state *after* executing
  it, so a node cannot forge a balance, a credential or a validator set without
  every other node detecting it by re-execution.
- **Fork choice by cumulative work**, with finalized history protected. An
  adopted chain is re-executed under *local* network parameters and its state
  rebuilt from genesis — a candidate's own claims about its genesis are never
  trusted.
- **Bounded difficulty (1–5)**, so a malformed difficulty can neither disable
  Proof of Work nor stall a node.

### 7.2 Consensus and Network Security

**Hashrate alone buys nothing.** Fork choice is validator-progress first: a fork
must carry *more validator-sealed blocks* to displace the local chain, and
cumulative Proof-of-Work only breaks exact ties. An attacker with unlimited
hashpower and no stake cannot reorganize the chain — which is the direct answer
to the "51% attack" question as usually asked, and a different security model
from pure Proof-of-Work.

**What an attacker with stake can do**, stated plainly: a majority of *staked*
HKM could censor transactions and reorganize unfinalized history. It could not
mint supply, forge a signature, or spend an account it does not hold keys for —
those are checked by every node independently. Finalized blocks are irreversible
regardless.

**Equivocation is punished, not merely detected.** Proofs are permissionless —
anyone may submit one — and burn the offender's stake on chain. Withdrawn stake
stays locked and slashable for the unbonding period, and the slashing window
equals it, so misbehaving stake can never exit ahead of its punishment.

**Nothing to grind.** VRF outputs are unique per (key, slot), so a validator
cannot search for a favourable value. The residual bias is the standard
Praos/RANDAO bound: a selected leader may *withhold* its block, forfeiting the
reward, to avoid contributing its randomness.

**Liveness.** A dead validator delays the chain by at most one slot timeout
(30 seconds), never stalls it, because each elapsed timeout opens the next
round's leader. Block timestamps are consensus-constrained in both directions —
never before the parent, bounded future skew — which closes difficulty-retarget
manipulation.

**Sybil resistance** comes from stake, not identity. Registering a validator
requires the minimum bond; at launch a genesis allowlist may gate who may join
at all, which is documented honestly as a permissioned posture rather than
described as decentralized.

**Eclipse and gossip.** Every envelope is signed by the sender's node key and
bound to its derived node id; `P2P_REQUIRE_IDENTITY=true` rejects unsigned
envelopes. Peer reputation scoring auto-bans repeat offenders and an optional
allowlist restricts participation. A bounded message-id cache provides replay
protection.

**Residual:** eclipse resistance ultimately depends on peer diversity, which is
an operational property rather than a protocol guarantee. Per-IP rate limiting
is not implemented; deploy behind a proxy that provides it.

### 7.3 Application and Node Security

**There is no contract execution environment to secure** (§4.1). The
application-layer surface is the set of transaction types, each validated
before any state is touched:

- **Recipients are validated.** A transfer to a malformed address is rejected
  rather than executed, because crediting a mistyped address puts funds where
  nobody holds a key. There is no checksum and no recovery; refusal is the only
  safe behaviour.
- **Arithmetic is checked, not wrapping.** Every balance and supply operation
  uses checked arithmetic, and the release profile enables overflow checks.
  This is not decoration: an unchecked fee addition once allowed 0.01 HKM to be
  turned into 184× the total supply.
- **Authorization is verified where state changes**, not where callers
  remember to ask. `apply_transaction` verifies the sender's signature itself,
  so a code path that forgets to check cannot admit a forged transaction.
- **Nonces are strictly sequential** per account — no reuse, no skipping ahead
  to reserve a slot.

**Ordering and MEV, honestly.** Within a block, transaction ordering is chosen
by that block's producer. That is true of every blockchain, and Hikmalayer
does not claim otherwise. What follows is that AMM trades should carry
slippage bounds — which the protocol requires as a field and the SDK refuses to
omit — and what does *not* follow is any claim of front-running immunity.

**API security:**

- **Deny-by-default authorization.** Admin endpoints require `x-admin-token`
  and P2P endpoints require `x-p2p-token`. An unset token **disables** the
  endpoint rather than opening it. Tokens support `*_TOKEN_CURRENT` /
  `*_TOKEN_PREVIOUS` rotation and are compared in constant time.
- **The node never accepts a private key**, on any endpoint, for any purpose.
  Signing happens in the wallet, the extension, or the offline CLI.
- **Cryptographic node identity.** Every gossip envelope is signed by the
  sender's node key and bound to its derived node id;
  `P2P_REQUIRE_IDENTITY=true` rejects any unsigned envelope. Peer reputation
  scoring auto-bans repeat offenders, and an optional `P2P_ALLOWLIST` restricts
  participation to named node ids.
- **Bounded resources.** The mempool is capped; explorer inputs are
  length-limited; difficulty is clamped.

**Wallet security:**

- Keys are generated in the browser and never leave it — never sent to a node,
  never placed in a URL, never logged.
- At rest they exist only as AES-256-GCM ciphertext under a PBKDF2-SHA256 key
  (310,000 iterations).
- While unlocked the key is held encrypted under a **non-extractable**
  per-session WebCrypto key and decrypted to a short-lived buffer only for the
  instant of signing, then wiped.
- Every signature requires explicit approval showing the exact canonical
  message, so a scripted signing spree is visible and refusable.
- The browser **extension** removes the main residual risk: keys live in the
  extension's own context, where a website — or an XSS in one — cannot reach
  them.

Honest limits: none of this defends against malware on the device. Treasury
and validator keys belong offline. See `docs/wallet_security.md`.

**Data protection:**

- **Minimal on-chain data.** Proof-of-Credential publishes a document *hash*,
  never the document.
- **Audit trail.** The chain itself is the log; every state transition is
  reconstructible from the block history.

### 7.4 Operational Security

**Deployment.**

- **TLS at the deployment layer.** The node speaks HTTP; terminate TLS at a
  reverse proxy, which is also where per-IP rate limiting belongs since the node
  does not implement it.
- **Admin and P2P tokens are deny-by-default**: unset means the endpoint is
  disabled, not open. Rotate via `*_TOKEN_CURRENT`/`*_TOKEN_PREVIOUS`;
  comparison is constant-time. Optionally, `mint_token` issues HMAC-signed,
  self-expiring, scope-bound tokens verified statelessly and fail-closed.
- **Genesis parameters are chain identity.** Chain id, supply, allowlist,
  hybrid requirement and the genesis validator's keys are all committed to by
  the genesis state root. Two nodes that disagree on any of them are running
  different networks — which is the intended behaviour, not a bug.
- **Validator keys** belong in an HSM or remote signer. The honest caveat: most
  cannot produce ML-DSA-65 signatures today, so a hybrid validator's key is
  currently a software key.

**Observability.** Metrics cover blocks mined, received and rejected, reorgs,
gossip volume, transactions, slashes, peers banned, and invalid messages from
peers; identity and enforcement settings are logged at startup so an operator
can see what is actually enabled rather than what they intended.

**Recovery.** State is persisted atomically and rebuilt by replay on startup,
and **rejected if replay fails** rather than trusted. `GET /snapshot` exports
the tip state with authenticating commitments; `GET /checkpoint` returns a
pinnable finalized (height, block hash, state root) triple for weak-subjectivity
anchoring.

**Maintenance.** Dependency and security updates; the adversarial suite
(`tests/security.rs`) is the regression gate for anything touching consensus or
cryptography, and every finding in `docs/security_assessment.md` has a test that
fails on the pre-fix code.

**Stated limits.** Admin tokens are bearer credentials, not signatures — anyone
holding one can drive those endpoints, so treat `ADMIN_TOKEN` as a production
secret. There is no user authentication system and none is planned: value-bearing
calls are authorized by signature, and administrative ones by operator-held
tokens. Those are the only two mechanisms, deliberately.

---

## 8. Performance

Numbers here are measured, not modelled, and the measurement conditions are
given so they can be reproduced or disputed.

### 8.1 What has been measured

**Execution-layer throughput** (Docker Compose: bootnode + validators + RPC +
Prometheus + Grafana; REST transaction harness; 600-second sustained run):

| Metric | Result |
|---|---|
| Total transactions | 8,940 |
| Average throughput | 14.88 TPS |
| Average latency | ~67 ms |
| Reorgs | 0 |
| Memory per node | ~4–5 MB |

**Scope, honestly.** This measures transaction execution and API handling under
sustained load in a local multi-container deployment. It is **not** a wide-area
consensus benchmark: it does not measure block propagation across a real
network, fork resolution under partition, or validator gossip at scale. Those
require a public testnet with independent operators, which is a launch
prerequisite (§9) and has not happened.

**Cryptographic costs** (single core, measured):

| Operation | Classical | Quantum-ready |
|---|---|---|
| Key derivation | <1 ms | ~3 ms (ML-DSA-65 keygen) |
| Signing | <1 ms | ~11 ms |
| Verification | <1 ms | ~4 ms |
| Bytes per transaction | ~130 | ~5,400 |

The post-quantum figures are the honest cost of §7.1.4 and the reason hybrid is
opt-in per account. A block full of hybrid transactions is roughly 40× larger
than the classical equivalent, which is a bandwidth and storage question long
before it is a CPU question.

### 8.2 Storage and state

State is **persistent**, not volatile. The chain is written to disk atomically
(temp file plus rename, so a crash mid-write cannot corrupt it) and state is
rebuilt by replaying blocks on startup — and rejected if replay fails, rather
than trusted.

- **Block storage** grows linearly with chain length.
- **State** is held in ordered maps, giving deterministic serialization — which
  is what makes the state root canonical across implementations.
- **Checkpoint fast-sync.** `GET /checkpoint/bundle` serves a self-verifying,
  retarget-boundary-anchored bundle; `HIKMALAYER_CHECKPOINT` boots a fresh node
  from it without full genesis replay, reconstructing a byte-identical state
  root, randomness beacon and difficulty. Full trust-minimizing replay remains
  the **default**; fast-sync is an explicit, opt-in weak-subjectivity assumption.

### 8.3 Bounds that exist by design

| Bound | Value | Why |
|---|---|---|
| Mempool | 1,000 transactions | A cap is a defence; an unbounded pool is a memory exhaustion vector |
| Transactions per block | 100 | Bounds validation work per block |
| Request body | 1 MiB | Bounds parse cost |
| Difficulty | clamped 1–5 | A malformed value can neither disable PoW nor stall a node |
| Target block time | 15 s, retargeted every 10 blocks | Deterministic per-chain schedule, consensus-validated |

Mining runs on the blocking thread pool with a tip-moved recheck, so a node
stays responsive to reads while it works.

### 8.4 What limits throughput, and what would raise it

Throughput is bounded by the per-block transaction cap and the block interval,
not by execution speed — execution is a state-machine step, not a VM. Raising
it is a parameter change with real consequences (larger blocks propagate more
slowly, which raises fork rates), and it is exactly the kind of change that
should be made against public-testnet measurements rather than local ones.

**Not implemented, and not claimed:** sharding, Layer-2 rollups, or state
pruning beyond checkpoint fast-sync.

---

## 9. Roadmap

Written as a list of what is genuinely undone. Anything already implemented is
described in the preceding sections and is not repeated here as a plan.

### 9.1 Before mainnet — launch blockers

1. **External security audit.** Independent review of consensus, cryptography,
   the state machine, P2P and node operations. This cannot be self-performed,
   and no real value should touch this chain before it is complete and its
   findings remediated. **Post-quantum expertise must be in scope**: the
   dual-hybrid scheme is the newest code in the system, and reviewing it means
   reviewing lattice-signature usage, the hybrid binding construction, and every
   downgrade path — not checking that a library was called. The engagement guide
   is `docs/external_audit_guide.md`.

2. **Public adversarial testnet.** Independent validators, real network
   conditions, and incentives to attack it. This is where wide-area consensus
   behaviour gets measured (§8.1) and where the permissioned launch posture gets
   tested before it is relaxed.

3. **Genesis distribution policy.** Who receives what from the genesis treasury,
   published as on-chain vesting schedules so it is verifiable rather than
   promised (§5.6). A business decision, not code.

4. **Production key management.** HSM or remote signer for validator keys. A
   real constraint applies here: most HSMs and remote signers cannot produce
   ML-DSA-65 signatures today, so a hybrid validator's key is currently a
   software key. That should factor into whether a given validator runs
   classical or hybrid, and it is a limitation of the wider ecosystem rather
   than of this chain.

### 9.2 Known gaps with no current answer

**Post-quantum leader election.** The sr25519 VRF remains classical (§7.1.5).
There is no standardized post-quantum VRF; the available options are a
hash-based construction with weaker unpredictability guarantees, or waiting for
standardization. Hikmalayer waits, and documents the gap rather than claiming
coverage it does not have. This is the single largest open cryptographic item.

**Post-quantum hardware signing.** Tracked separately from the above because it
is an ecosystem dependency, not a design choice.

**No seed phrase / HD derivation.** Each account key is independent and needs its
own backup. This is a genuine usability cost.

**Opening the validator set.** `GENESIS_VALIDATOR_ALLOWLIST` gates who may join
at launch. Removing it is a governance decision with real security
consequences — a small permissionless validator set is cheap to outvote — and
should follow the adversarial testnet, not precede it.

### 9.3 Under consideration, not committed

- **Threshold or multi-signature accounts**, which would give treasuries a
  protocol-level answer rather than an operational one.
- **Raising throughput bounds** against public-testnet measurements (§8.4).
- **Credential schema conventions** — how issuers describe what a credential
  hash represents, without putting the document on chain.

### 9.4 Explicitly not pursued

**A cross-chain bridge.** Hikmalayer will not custody external assets. Bridges
are the most-attacked component in the industry (Ronin $600M, Wormhole $320M,
Nomad $190M); declining one removes that attack surface entirely, along with a
dedicated audit budget, 24/7 signer operations and custody-related legal
exposure. The cost is stated honestly in `docs/hts_and_listings.md`: without a
bridge, a centralized exchange must integrate Hikmalayer natively, which is
bespoke work that raises the bar for listing. That trade was made deliberately.
No wrapped or external asset is or will be tradeable on this chain, and no
public material should suggest otherwise. Reasoning: `docs/bridge_design.md`.

**A general virtual machine.** §4.1 explains the trade. Adding one later would
reintroduce precisely the attack surface this design excludes, and would be a
different chain rather than an upgrade to this one.

---

## 10. What Hikmalayer Contributes

### 10.1 The claims, and what backs them

Four things distinguish this chain. Each is stated with what would falsify it.

**1. Dual-hybrid cryptography, enforced everywhere a key authorizes something.**
Plenty of projects announce post-quantum intentions. What matters is whether the
guarantee holds on the paths that carry value rather than only on a transfer.
Here it holds on transfers, tokens, the AMM, vesting, credentials, staking,
**unbonding**, and **block production** — and the address, not the transaction,
decides which scheme applies, so a hybrid account cannot be downgraded.
*Falsifiable by:* finding any path where a hybrid account's value moves on a
single signature. `tests/security.rs` contains the attempts, each assuming the
attacker already holds the victim's secp256k1 key.

**2. Hybrid consensus where hashrate without stake is worthless.** Leader
selection is stake-weighted and VRF-seeded; finalization is Proof-of-Work by
that leader; fork choice counts validator-sealed blocks first and uses work only
to break ties. *Falsifiable by:* reorganizing the chain with hashpower alone.

**3. Capabilities as consensus objects, not contracts.** HTS tokens, the AMM,
vesting and credentials are executed by the state machine. Every HTS token has a
fixed supply, no mint function, no blacklist, no transfer hook and no
upgradeable proxy — guaranteed by consensus rather than by an audit of that
token's code. The cost is equally real and stated throughout: no arbitrary
applications, and no legitimate custom token behaviour either.

**4. Credentials that publish a hash, not a document.** Proof-of-Credential
issues, revokes and verifies against the block-committed state root, so a
verifier trusts arithmetic rather than a node — and the credential's contents
never go on chain at all.

### 10.2 What this document does not claim

- **That the cryptography has been independently reviewed.** It has not. The
  13 findings in `docs/security_assessment.md` were found by the people who
  wrote the code, which is exactly why an external audit is a launch blocker
  (§9.1) and not a formality.
- **That leader election is quantum-safe.** The sr25519 VRF is classical, and
  §7.1.5 says what that does and does not expose.
- **That this is decentralized today.** A genesis allowlist may gate validator
  registration at launch. That is a permissioned posture, described as one.
- **That HTS tokens will trade outside Hikmalayer.** There is no bridge, so a
  centralized listing requires bespoke integration. `docs/hts_and_listings.md`
  sets out honest expectations rather than optimistic ones.
- **That measured throughput reflects a real network.** §8.1 was measured in a
  local multi-container deployment; wide-area consensus behaviour needs a public
  testnet.

### 10.3 Why the trade-offs were made this way

Every design decision here has a cost, and the costs were chosen deliberately:

| Decision | What it buys | What it costs |
|---|---|---|
| Hybrid signatures | Survives a break of either scheme | ~40× transaction size, ~11 ms signing |
| No virtual machine | No contract-bug attack surface | No arbitrary applications |
| No bridge | Removes the industry's most-attacked component | Harder path to exchange listings |
| Fixed-supply HTS | No token can secretly inflate | No legitimate custom behaviour either |
| PoW finalization on top of PoS | Cost to produce; stake gates who may | Energy use per block |
| Permissioned launch | A small validator set is cheap to outvote | Not decentralized on day one |

A reader who disagrees with any row is disagreeing with a decision, not
discovering an oversight. That is the intent: the trade-offs are visible so they
can be argued with.

### 10.4 Position

Hikmalayer is a Layer 1 built for durability rather than breadth. It does one
class of thing — verifiable credentials and native assets — and it does that
with cryptography chosen to still be standing when secp256k1 is not.

The honest summary is that the protocol is built and tested, the trade-offs are
deliberate and documented, and what remains before mainnet is external
validation rather than missing code. That is a good position to be in, and it is
not the same thing as being finished.

---

## 11. Technical Specifications

### 11.1 System Requirements

**Server Infrastructure:**

- **Operating System**: Linux (Ubuntu 20.04+ recommended), macOS, or Windows 10+
- **Processor**: x86_64 architecture, minimum 2 CPU cores (4+ recommended for production)
- **Memory**: Minimum 512MB RAM (2GB+ recommended for production workloads)
- **Storage**: 1GB available disk space for initial deployment (scaling with blockchain growth)
- **Network**: Stable internet connection with open HTTP/HTTPS ports

**Development Environment:**

- **Rust**: 1.75 or later with Cargo
- **Node.js**: 20 or later, for the SDK, dashboard and extension
- **Core dependencies**: Tokio (async runtime), Axum (HTTP), Serde
  (serialization), `secp256k1` (ECDSA), `schnorrkel` (sr25519 VRF), `fips204`
  (ML-DSA-65), `sha2`/`sha3` (hashing)
- **Client cryptography**: `@noble/curves`, `@noble/hashes`,
  `@noble/post-quantum` — chosen so the browser and the node produce
  byte-identical signatures, which is asserted by test
- **Testing**: 139 Rust unit tests, 40 adversarial tests (`tests/security.rs`),
  58 SDK offline tests including Rust↔JS byte parity, 21 live integration tests
  against a running chain, plus an in-browser verification. `cargo clippy
  --all-targets -- -D warnings` is clean

**Client Requirements:**

- **API Access**: Any HTTP client capable of JSON communication
- **Web Integration**: Modern web browsers supporting JavaScript ES6+ and Fetch API
- **Mobile Integration**: Native mobile apps or mobile web applications with HTTP capabilities
- **Enterprise Integration**: REST API compatible enterprise software systems

### 11.2 API Specifications

**Authentication and Authorization:**

- **Value-bearing calls are signature-authorized, not session-authorized.**
  There is no login. A transfer, stake, token operation or trade carries the
  sender's public key and signature (and, for a `hkq…` sender, an ML-DSA public
  key and signature as well); the node reconstructs the canonical message from
  the request fields and verifies against it. A request the signature does not
  cover is not a request.
- **Administrative endpoints are token-gated**, deny-by-default: the faucet,
  mining trigger, difficulty and governance endpoints require `x-admin-token`,
  and are **disabled** when the token is unset. P2P endpoints require
  `x-p2p-token` on the same terms.
- **Token rotation** is supported via `*_TOKEN_CURRENT` / `*_TOKEN_PREVIOUS`,
  and comparison is constant-time.
- **The API never accepts a private key.**

Honest limitation: admin tokens are bearer credentials, not signatures. Anyone
holding `ADMIN_TOKEN` can drive those endpoints, so it must be treated as a
production secret.

**Rate Limiting:**

- **Implemented structurally**: the mempool is capped, oversized inputs are
  rejected, and the projected-state check makes an inapplicable transaction
  cost one verification rather than a scan of the pool.
- **Not implemented**: per-IP request rate limiting. Deploy behind a reverse
  proxy that provides it.

**API Versioning:**

- **Current**: Version 1.0 API with stable endpoint contracts
- **Future**: Semantic versioning with backward compatibility guarantees
- **Migration**: Clear migration paths for API version updates

### 11.3 Data Formats and Standards

**JSON Schema Compliance:**
All API endpoints use standardized JSON schemas for request and response formats:

**Certificate Request Schema:**

```json
{
  "type": "object",
  "properties": {
    "id": { "type": "string", "minLength": 1, "maxLength": 100 },
    "issued_to": { "type": "string", "minLength": 1, "maxLength": 200 },
    "description": { "type": "string", "minLength": 1, "maxLength": 500 }
  },
  "required": ["id", "issued_to", "description"]
}
```

**Token Transfer Schema:**

```json
{
  "type": "object",
  "properties": {
    "from": { "type": "string", "minLength": 1, "maxLength": 100 },
    "to": { "type": "string", "minLength": 1, "maxLength": 100 },
    "amount": { "type": "integer", "minimum": 1 }
  },
  "required": ["from", "to", "amount"]
}
```

**Blockchain Data Standards:**

- **Hash Format**: SHA-256 hexadecimal representation (64 characters)
- **Timestamp Format**: ISO 8601 UTC timestamps (YYYY-MM-DDTHH:MM:SS.sssssssssZ)
- **UUID Format**: RFC 4122 compliant UUID version 4 for transaction identifiers
- **Address Format**: String-based account identifiers (alphanumeric, 1-100 characters)

### 11.4 Cryptographic Specifications

| Purpose | Algorithm | Standard |
|---|---|---|
| Hashing, addresses, Merkle roots, PoW | SHA-256 | FIPS 180-4 |
| Classical signatures | secp256k1 ECDSA, compact `r‖s`, low-S normalized | SEC 1 / SEC 2 |
| Post-quantum signatures | ML-DSA-65 | **FIPS 204**, NIST security category 3 |
| Leader-election randomness | sr25519 VRF (Ristretto255, schnorrkel) | — |
| Wallet vault | AES-256-GCM | NIST SP 800-38D |
| Vault key derivation | PBKDF2-HMAC-SHA256, 310,000 iterations | NIST SP 800-132 (OWASP-recommended count) |
| Randomness | Platform CSPRNG (`getrandom` / WebCrypto) | — |

**Sizes:**

| | Public key | Signature |
|---|---|---|
| secp256k1 | 65 bytes (uncompressed, canonical) | 64 bytes |
| ML-DSA-65 | 1,952 bytes | 3,309 bytes |

**Domain separation.** Every signed context has its own prefix, so a signature
made in one role is meaningless in another:

| Context | Separator |
|---|---|
| Account messages | `\x19Hikmalayer Signed Message:\n` + byte length |
| Network scope | `<chain_id>:` prefixed into the message |
| Operation | `hikmalayer-transfer:`, `hikmalayer-stake:`, … |
| Block signature (classical) | signs the raw 32-byte hash, not a prefixed message |
| Block signature (post-quantum) | `hikmalayer-pq-block-v1:` |
| ML-DSA key derivation | `hikmalayer-pq-key-v1` |
| ML-DSA signing seed | `hikmalayer-pq-sign-v1` |
| Hybrid address | `hikmalayer-hybrid-address-v1` |
| FIPS 204 context string | `hikmalayer` |

**Canonical encodings.** Public keys are accepted in exactly one form —
uncompressed, 65 bytes, `04`-prefixed, lower-case hex (ML-DSA keys: lower-case
hex). One authorized transaction therefore has exactly one valid on-wire form
and one transaction id.

**Network security:** configurable CORS; TLS terminated at the deployment layer;
input validation and length bounds throughout; error messages written to avoid
disclosing internal state.

---

## 12. Governance and Compliance

### 12.1 Governance

**What is on chain today.** Two governance-relevant mechanisms are implemented
and consensus-enforced, and they are the only ones:

- **Runtime parameters** (finality depth and related settings) are held in a
  governance configuration and changed through admin-gated endpoints.
- **The validator allowlist**, when configured, is baked into the genesis state
  root and gates who may *join* the validator set.

Everything else about how this project is run is off-chain, and it is more
useful to say so than to describe a governance system that does not exist.

**What is off chain today.** Development is led by Bestower Labs Limited.
Protocol changes are made by that team, published in this repository, and
adopted by operators choosing to run the software. That is a centralized
development model. Calling it anything else would be inaccurate.

**Stake is not a vote.** There is no on-chain proposal system, no token-weighted
voting, and no treasury governance contract. A validator's influence is over
block production, not over protocol rules — and since rules are enforced by
every node re-executing every block, a validator that changes them is running a
different chain, not amending this one.

**Upgrades are operator adoption.** With no on-chain upgrade mechanism, a
protocol change ships as software that operators choose to run. A change that
alters consensus is a hard fork by definition, and nodes that do not adopt it
will reject the resulting blocks. This is a real constraint on how fast the
protocol can move, and it is the correct constraint for a chain holding value.

**Direction, stated as intent rather than commitment.** Opening the validator
set — removing the genesis allowlist — is the first meaningful decentralization
step, and it should follow the adversarial testnet rather than precede it (§9.1),
because a small permissionless validator set is cheap to outvote. Beyond that,
on-chain governance is a design question this document does not pretend to have
answered. Publishing a dated roadmap toward a DAO would be a promise rather than
a plan, and this whitepaper avoids those.

**What holders should understand.** HKM is a network asset: it pays fees, secures
the chain through staking, and earns block rewards. It is **not** a governance
token, confers no voting right, and no mechanism exists in the protocol by which
it could. Nothing in this document is an offer, a solicitation, or investment
advice.

### 12.2 Data Protection

The most important fact about Hikmalayer and data protection is architectural:
**no personal data goes on chain.**

Proof-of-Credential publishes a **hash** of a credential document, an issuer
address, a subject identifier, a revocation flag and a height. The document —
the name, the grade, the qualification, the photograph — never touches the
chain. A hash of a document is not the document, and it does not become the
document by being on a blockchain.

That is what makes the rest of this section tractable rather than aspirational.

**On erasure, stated plainly.** A blockchain cannot delete history; any claim
otherwise is false. Hikmalayer's answer is not a deletion mechanism, because
there is nothing on chain to delete:

- The credential document lives with the issuer or the subject, under whatever
  retention and erasure obligations apply there. It can be deleted, and when it
  is, the on-chain hash becomes a commitment to a document nobody holds.
- **Revocation is a first-class on-chain operation**, so an issuer can withdraw
  a credential's validity immediately and verifiably — which is the operative
  remedy in practice, and it is enforced by consensus rather than by policy.
- A **subject identifier** is chosen by the issuer. Deployments handling
  personal data should use a pseudonymous identifier rather than a name or a
  national number, exactly as they would in any append-only log.

**Data minimization** is therefore not a policy commitment but a property of the
design: the protocol has no field in which to put personal data even if an
operator wanted to.

**What deployers remain responsible for.** Hikmalayer is infrastructure, not a
compliance product. An organization issuing credentials is the controller of the
underlying documents and remains responsible for lawful basis, consent, subject
access, retention, and cross-border transfer of everything it holds off chain.
Nothing in this document is legal advice, and no blockchain design discharges
those duties.

**Addresses are pseudonymous, not anonymous.** An address is not a name, but a
chain is a permanent public record of its activity, and analysis can link
addresses to identities. Anyone treating an address as anonymous is mistaken.

**Not implemented, and not claimed:** on-chain identity attestation of natural
persons, selective-disclosure or zero-knowledge credential proofs, encrypted
on-chain payloads, and any form of on-chain personal-data storage. If a
deployment needs selective disclosure, it belongs in the credential format the
issuer and verifier exchange off chain — the hash commitment works unchanged.

### 12.3 Standards

Split into what the implementation **conforms to** — verifiable by reading the
code — and what it is merely **compatible with in principle**. Conflating the
two is how whitepapers mislead.

**Conformed to, in the implementation:**

| Standard | Where |
|---|---|
| **FIPS 204** — Module-Lattice-Based Digital Signature Standard (ML-DSA) | Post-quantum signatures, ML-DSA-65 parameter set, with the specified context string and deterministic seed |
| **FIPS 180-4** — SHA-2 | Hashing, addresses, Merkle roots, Proof-of-Work |
| **SEC 1 / SEC 2** | secp256k1 ECDSA, compact `r‖s`, low-S normalized |
| **NIST SP 800-38D** | AES-256-GCM wallet vaults |
| **NIST SP 800-132** | PBKDF2-HMAC-SHA256 key derivation (310,000 iterations, per OWASP guidance) |
| **OpenAPI 3.1** | `docs/openapi.yaml`, lints clean |
| **RFC 7515-adjacent** | Constant-time comparison of bearer tokens |

**Compatible in principle, not implemented:**

- **W3C Verifiable Credentials.** Hikmalayer's credential records are hash
  commitments, not VC documents. A VC can be anchored here by hashing it — the
  chain neither knows nor cares about the format — but Hikmalayer does not parse,
  validate or emit VC JSON-LD, and claiming conformance would be wrong.
- **W3C Decentralized Identifiers (DID).** A `hkm…`/`hkq…` address could back a
  DID method. No DID method is registered, specified or implemented.
- **Open Badges, PESC, IMS Global.** Any of these can be anchored by hash. None
  is implemented as a format.

**Not claimed:** membership in or contribution to IEEE, ISO or W3C working
groups; ISO 27001 or ISO 31000 certification; any third-party audit or
attestation. An earlier version of this document implied some of these. It
should not have, and this section replaces it.

### 12.4 Environmental and Ethical Considerations

**Energy, without euphemism.** Hikmalayer uses Proof of Work, and Proof of Work
consumes energy. Two things make its footprint structurally different from
Bitcoin's, and one thing does not:

- **Only the selected leader mines.** There is no global race: a single
  validator, chosen by stake and VRF, mines each block. The industry-wide
  pattern where thousands of machines burn energy to lose the same race does not
  occur here, because there is no race to lose.
- **Difficulty is clamped to 1–5 hex zeros** and retargeted to a 15-second
  block, so the work per block is bounded by consensus rather than by whatever
  hardware happens to be pointed at the chain.
- **It is still not zero.** Work is the point — it is what makes a block costly
  to produce. A chain that wanted zero energy would drop Proof of Work, and
  would be a different design with different guarantees.

Post-quantum signatures add their own cost: roughly 40× the bytes and ~11 ms of
CPU per hybrid transaction (§8.1). Bandwidth and storage, not electricity, are
the dominant term there. It is a real cost and it is why hybrid is opt-in.

**Accessibility.** A node runs in a few megabytes of memory and needs no
specialized hardware, which keeps participation cheap. Validators must meet the
10,000 HKM minimum stake — a deliberate anti-spam floor that is also, honestly,
a barrier to entry. The API is plain JSON over HTTP; the SDK, the CLI and the
browser wallets are the intended integration paths.

**Where an honest limit belongs.** The wallet's security model protects against
websites and passive scraping; it does not protect a compromised device.
Credential issuers hold real power over subjects, and the chain enforces
*revocation* rather than *fairness* — the protocol cannot tell a legitimate
revocation from a retaliatory one, and should not be described as though it
could.

**Not claimed:** carbon accounting or offsetting, accessibility conformance
(WCAG or otherwise), or multi-language support. None of these is implemented,
and listing them as commitments would be marketing rather than description.

---

## 13. Risk Assessment and Mitigation

### 13.1 Technical Risks

**Blockchain-Specific Risks:**

**Consensus Attack Risks:**

- **Risk**: 51% attacks or consensus manipulation
- **Impact**: High - could compromise blockchain integrity
- **Probability**: Low in current centralized context, moderate as network grows
- **Mitigation**: Distributed mining, monitoring systems, rapid response procedures

**Quantum Computing Risk:**

- **Risk**: A cryptographically relevant quantum computer recovers private keys
  from the public keys this chain publishes with every transaction, and from
  validator keys that sit permanently in the staker set
- **Impact**: **Critical** for classical (`hkm…`) accounts — total loss of
  control. **Low** for quantum-ready (`hkq…`) accounts, which additionally
  require an ML-DSA-65 signature (§7.1.4)
- **Probability**: Timing unknowable. "Harvest now, decrypt later" means the
  archive is already being built, so the decision cannot be deferred to the
  moment the hardware appears
- **Mitigation**: Hybrid accounts, available now, enforced across transfers,
  tokens, the AMM, staking, unbonding and block production; a network may
  require them at genesis. **Residual**: the sr25519 VRF remains classical
  (§7.1.5), and accounts that do not migrate keep the classical exposure —
  migration is a user action, and no protocol can perform it on their behalf

**Post-Quantum Algorithm Risk:**

- **Risk**: A classical break of ML-DSA, which is a much younger scheme than
  secp256k1
- **Impact**: Low — hybrid accounts also require ECDSA, so an ML-DSA break
  alone forges nothing
- **Probability**: Low but not negligible; lattice cryptanalysis is an active
  field
- **Mitigation**: This is precisely why the scheme is hybrid rather than a
  replacement. Both schemes must fall for an account to fall

**Absence-of-VM Risk (accepted trade):**

- **Risk**: No user-deployable contracts limits what third parties can build,
  and new capabilities require protocol changes rather than deployments
- **Impact**: Medium for ecosystem growth
- **Probability**: Certain — it is a design decision, not a failure mode
- **Mitigation**: Rich protocol-native primitives (HTS, AMM, vesting,
  credentials) plus an SDK, REST API and wallet tooling. Accepted deliberately:
  the majority of value ever stolen from blockchains was stolen through
  contract bugs, not consensus bugs (§4.1)

**Scalability Risks:**

- **Risk**: Chain growth degrades sync time and storage; throughput is bounded
  by the per-block transaction cap and block interval, not by execution speed
- **Impact**: Medium — affects new-node onboarding before it affects users
- **Probability**: Certain over a long enough horizon
- **Mitigation**: Checkpoint fast-sync exists (self-verifying, boundary-anchored,
  reconstructing a byte-identical state root) with full replay as the default.
  Raising throughput bounds is a parameter change that should be made against
  public-testnet measurements, not local ones (§8.4). **Not implemented:**
  sharding, Layer-2 rollups, or pruning beyond checkpoints

**Key Management Risks:**

- **Risk**: Loss or compromise of validator, treasury or admin keys
- **Impact**: **Critical** for a treasury or validator key — a stolen validator
  key can sign blocks and unbond stake; a lost account key means unrecoverable
  funds, because a recovery mechanism would itself be an attack surface
- **Probability**: Medium, and rising with the number of holders
- **Mitigation**: The node never accepts a private key on any endpoint. Cold
  keys sign offline via `hikma-wallet`; the extension keeps keys out of web
  pages; vaults are AES-256-GCM under PBKDF2-SHA256. Admin tokens rotate
  without downtime. Equivocation is slashable, which limits but does not undo
  validator-key theft. **Residual:** there is no multi-signature or threshold
  account type (§9.3), no seed phrase, and — for hybrid validators — no HSM that
  can produce ML-DSA-65 signatures today, so those keys are software keys

**Client Implementation Risk:**

- **Risk**: The JavaScript and Rust signers drift, so browser-signed
  transactions are silently refused on chain
- **Impact**: Medium — a total loss of usability for affected clients, with an
  error message ("signature verification failed") that names nothing useful
- **Probability**: Low but real; it is exactly the failure mode determinism was
  chosen to make detectable
- **Mitigation**: Conformance tests assert **byte-identical** keys and
  signatures against the real CLI signer across every message domain, including
  non-ASCII cases and both post-quantum halves, plus an in-browser check. A
  drift fails the test suite rather than production

### 13.2 Operational Risks

**Availability:**

- **Risk**: A node or RPC endpoint goes down
- **Impact**: **Low for the chain, high for that operator's users.** The chain
  is replicated: other validators continue producing, and an offline validator
  delays the chain by at most one slot timeout
- **Mitigation**: Run redundant RPC nodes; monitor the metrics the node already
  exposes. This is an operator concern, not a protocol one

**Data loss:**

- **Risk**: An operator loses their chain database
- **Impact**: **Low.** The chain is replicated across every node; a lost
  database is re-synced, not reconstructed. State is a deterministic function of
  the blocks
- **Mitigation**: Re-sync from peers, or boot from a checkpoint bundle. State is
  written atomically so a crash mid-write cannot corrupt it
- **The genuinely unrecoverable case is a lost private key**, which is §13.1

**Genesis misconfiguration:**

- **Risk**: Chain id, supply, allowlist, hybrid requirement or genesis validator
  keys set incorrectly
- **Impact**: **High and unfixable after launch.** These are committed to by the
  genesis state root, so getting one wrong means you have defined a different
  network — nodes will not sync and signatures will not verify across the split
- **Probability**: Medium; it is a one-shot configuration with no feedback loop
- **Mitigation**: Genesis parameters are documented in one place
  (`docs/deployment_guide.md`) and verified at startup. A hybrid genesis
  validator without a matching ML-DSA key is **refused seating** rather than
  seated weakly — a loud failure by design

**Dependency risk:**

- **Risk**: A cryptographic dependency has a defect
- **Impact**: **Critical** — this is the highest-consequence dependency class,
  covering `secp256k1`, `schnorrkel`, `fips204` and the `@noble` libraries
- **Probability**: Low, but `fips204` and `@noble/post-quantum` implement a
  standard finalized in 2024, so they have less field exposure than the
  classical stack
- **Mitigation**: Widely used implementations rather than bespoke cryptography;
  cross-implementation byte-parity tests between Rust and JavaScript, which
  would catch a divergence in either; dependency monitoring. **Residual:** a
  defect present in *both* implementations of the same algorithm would not be
  caught by parity testing

**Human error:**

- **Risk**: Operational mistakes — leaked tokens, wrong environment, unset
  variables
- **Impact**: Variable
- **Mitigation**: Deny-by-default is the structural answer: an unset admin or
  P2P token *disables* the endpoint rather than opening it, so the failure mode
  of forgetting is closed, not open. Tokens rotate without downtime

### 13.3 Security Risks

Complementing `docs/threat_model.md`, which enumerates adversaries in full.

**Denial of service:**

- **Risk**: Volumetric or application-layer attacks on public endpoints
- **Impact**: Medium — service disruption; no data loss and no consensus effect
- **Mitigation**: Structural bounds are in the protocol (mempool cap, per-block
  transaction cap, 1 MiB bodies, and an inapplicable transaction costs one
  verification rather than a scan of the pool). **Per-IP rate limiting is not
  implemented** — deploy behind a proxy that provides it

**"Data breach":**

- **Risk**: Unauthorized access to node data
- **Impact**: **Low, and worth explaining.** There is nothing confidential on
  chain: balances, credentials-as-hashes and the validator set are public by
  design, and the node holds no private keys but its own. A credential's
  *document* is never on chain, so it cannot be breached from here
- **The real exposure is the admin token**, which is a bearer credential and can
  drive faucet, mining, difficulty and governance endpoints. Treat it as a
  production secret and rotate it

**Social engineering:**

- **Risk**: Manipulating an operator or key holder
- **Impact**: High — this is how most real losses happen, in this industry and
  others
- **Mitigation**: Signing is never silent: every wallet signature requires
  approval of the exact canonical message, so "just click approve" at least
  shows what is being approved. Cold keys belong offline. **Residual:** no
  protocol defeats a person who is deceived into authorizing something, and
  there is no multi-signature account type yet (§9.3)

**Supply chain:**

- **Risk**: A compromised dependency, build tool or published artifact
- **Impact**: **Critical** — it reaches consensus code and wallet code alike
- **Probability**: Low, rising industry-wide
- **Mitigation**: Locked dependency versions; reproducible `cargo` and `npm`
  builds; the browser extension is built from the same audited source as the
  in-page wallet. **Residual:** the extension has not been published or
  externally reviewed, and it is the component users trust with their keys

**Quantum adversary:** covered in §13.1 and §7.1.3–7.1.5, and given its own
section in `docs/threat_model.md`.

### 13.4 Business and Adoption Risks

**Market Adoption Risks:**

- **Risk**: Slow adoption by educational institutions and employers
- **Impact**: High - affects platform viability and sustainability
- **Probability**: Medium - typical for new technology platforms
- **Mitigation**: User education, pilot programs, integration support, clear value propositions

**Competitive Risks:**

- **Risk**: Competition from established players or new technologies
- **Impact**: Medium to High - could limit market share and growth
- **Probability**: High - active area of technology development
- **Mitigation**: Innovation focus, unique value propositions, strong community building

**Regulatory Risks:**

- **Risk**: Changes in regulations affecting blockchain or credential systems
- **Impact**: High - could require significant system changes or limit adoption
- **Probability**: Medium - regulatory landscape continues evolving
- **Mitigation**: Regulatory monitoring, compliance design, flexible architecture

**Technology Evolution Risks:**

- **Risk**: Emergence of superior technologies or standards
- **Impact**: Medium to High - could make platform obsolete
- **Probability**: Medium - natural technology evolution
- **Mitigation**: Continuous innovation, standard adoption, platform flexibility

### 13.5 How risk is actually managed here

The general risk-management vocabulary — monitoring, diversification, insurance
— is not what protects this system. Four concrete mechanisms do, and they are
worth naming instead:

**1. Structural defaults that fail closed.** An unset admin token disables the
endpoint. A replay that fails is rejected rather than trusted. A hybrid genesis
validator without its ML-DSA key is refused seating rather than seated weakly. A
malformed recipient is rejected rather than credited. In each case the failure
mode of forgetting something is *closed*, not open. This is the cheapest form of
risk management available and the most reliable.

**2. An adversarial test suite as the regression gate.** `tests/security.rs`
plays an attacker with a specific goal — mint supply, spend someone else's
funds, replay a signature, drain a pool, downgrade a hybrid account — and
asserts the chain refuses. Every finding in `docs/security_assessment.md` has a
test that **fails on the pre-fix code**, so a regression is caught by CI rather
than by a user.

**3. Cross-implementation parity.** The Rust node and the JavaScript clients
must produce byte-identical keys and signatures, asserted against the real
signer. This catches the failure mode that has no useful error message.

**4. External review, as a gate rather than a formality.** No independent audit
has been performed. That is the largest open risk in this document, it is a
launch blocker (§9.1), and no amount of internal testing substitutes for it —
including the testing described above, which was written by the same people who
wrote the code.

**What is not in place:** insurance, a bug bounty, a formal incident-response
retainer, or a security contact process. For a chain holding real value these
belong alongside the audit, and none of them is a substitute for it either.

---

## References and Further Reading

**Technical Documentation:**

- Rust Programming Language Official Documentation: https://doc.rust-lang.org/
- Axum Web Framework Documentation: https://docs.rs/axum/
- SHA-256 Cryptographic Standard: FIPS PUB 180-4
- Tokio Asynchronous Runtime: https://tokio.rs/

**Blockchain and Cryptography:**

- Nakamoto, S. (2008). "Bitcoin: A Peer-to-Peer Electronic Cash System"
- Merkle, R.C. (1987). "A Digital Signature Based on a Conventional Encryption Function"
- Lamport, L. (1979). "Constructing Digital Signatures from a One-Way Function"
- Wood, G. (2014). "Ethereum: A Secure Decentralised Generalised Transaction Ledger"
- David, B., Gaži, P., Kiayias, A., Russell, A. (2018). "Ouroboros Praos:
  An Adaptively-Secure, Semi-synchronous Proof-of-Stake Blockchain" — the VRF
  leader-election and withhold-bias model this chain follows
- Micali, S., Rabin, M., Vadhan, S. (1999). "Verifiable Random Functions"

**Post-Quantum Cryptography:**

- **NIST FIPS 204 (2024): Module-Lattice-Based Digital Signature Standard
  (ML-DSA)** — the post-quantum signature scheme implemented here
- NIST FIPS 203 (2024): Module-Lattice-Based Key-Encapsulation Mechanism (ML-KEM)
- Shor, P.W. (1994). "Algorithms for Quantum Computation: Discrete Logarithms
  and Factoring" — why secp256k1 and sr25519 are not durable
- Grover, L.K. (1996). "A Fast Quantum Mechanical Algorithm for Database
  Search" — why SHA-256 is
- Bindel, N., Herath, U., McKague, M., Stebila, D. (2017). "Transitioning to a
  Quantum-Resistant Public Key Infrastructure" — hybrid signature combiners
- NIST IR 8547 (draft): Transition to Post-Quantum Cryptography Standards
- Mosca, M. (2018). "Cybersecurity in an Era with Quantum Computers: Will We Be
  Ready?" — the "harvest now, decrypt later" timing argument

**Digital Credentials and Standards:**

- W3C Verifiable Credentials Data Model: https://www.w3.org/TR/vc-data-model/
- Mozilla Open Badges Specification: https://openbadges.org/
- IMS Global Learning Consortium Standards: https://www.imsglobal.org/
- IEEE Standards for Blockchain: https://standards.ieee.org/industry-connections/blockchain/

**Privacy and Security:**

- General Data Protection Regulation (GDPR): EU 2016/679
- California Consumer Privacy Act (CCPA): California Civil Code Section 1798.100
- ISO/IEC 27001:2013 Information Security Management
- NIST Cybersecurity Framework: https://www.nist.gov/cyberframework

**Educational Technology:**

- EDUCAUSE Research on Digital Credentials
- Pew Research Center: "The Future of Jobs and Education"
- MIT Technology Review: "Blockchain in Education"
- Chronicle of Higher Education: "Digital Credentialing Trends"

---

**Document Information:**

- **Title**: Hikmalayer: A Multi-Purpose Blockchain Platform for Digital Certificates and Tokenized Assets
- **Version**: 1.0
- **Date**: August 2025
- **Authors**: Mr. Muhammad Ayan Rao, Director, Bestower Labs Limited
- **Licenses**:
  - HikmaLayer Business Source License 1.1 (protocol source code, see `LICENSE`)
  - HikmaLayer Contributor License Agreement (see `CLA.md`)
  - Creative Commons Attribution 4.0 International (this whitepaper)
- **Contact**: legal@bestowerlabs.com

**Disclaimer:**
This whitepaper is provided for informational purposes only and does not constitute financial, legal, or investment advice. The technical specifications and roadmap outlined in this document are subject to change based on development progress, community feedback, and market conditions. Readers should conduct their own research and consult appropriate professionals before making decisions based on the information contained in this document.
