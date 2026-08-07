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

### 6.1 Security Features

**Cryptographic Security:**

- SHA-256 hashing for block integrity
- Proof-of-work prevents unauthorized modifications
- Input validation prevents injection attacks
- CORS configuration for secure web integration

**Operational Security:**

- Thread-safe concurrent access
- Comprehensive error handling
- API input sanitization
- Future support for HTTPS and authentication

### 6.2 Performance Characteristics

**Throughput:**

- Fast API response times (sub-millisecond for reads)
- Efficient in-memory data structures
- Concurrent request handling via Tokio async runtime
- Mining performance scales with available CPU power

**Scalability:**

- Linear memory growth with blockchain size
- Horizontal scaling support through API layer
- Future persistent storage integration planned
- Load balancing compatible architecture

---

## 7. Development Roadmap

### 7.1 Phase 1: Foundation (Q4 2025)

- **Persistent Storage**: Database backend for blockchain data
- **Enhanced Security**: Digital signatures and authentication
- **Performance Optimization**: Database indexing and caching

### 7.2 Phase 2: Network Expansion (Q1-Q2 2026)

- **Multi-Node Support**: Distributed blockchain network
- **Advanced Contracts**: Extended smart contract capabilities
- **Mobile SDKs**: Native mobile application support

### 7.3 Phase 3: Enterprise Integration (Q3-Q4 2026)

- **Enterprise Connectors**: Pre-built system integrations
- **Compliance Tools**: Enhanced audit and reporting features
- **Cross-Chain Bridges**: Interoperability with other blockchains

### 7.4 Future Vision (2027+)

- **Decentralized Governance**: Community-driven development
- **Advanced Privacy**: Zero-knowledge proof integration
- **IoT Integration**: Blockchain for Internet of Things applications

---

## 8. Technical Specifications

### 8.1 System Requirements

- **Server**: Linux/macOS/Windows, 2+ CPU cores, 512MB+ RAM
- **Development**: Rust 1.70+, Cargo, Tokio runtime
- **Client**: Any HTTP client with JSON support
- **Network**: Open HTTP ports, stable internet connection

### 8.2 Key Dependencies

- **Axum**: Web framework for REST API
- **Tokio**: Async runtime for concurrent processing
- **Serde**: JSON serialization and deserialization
- **SHA2**: Cryptographic hashing implementation
- **Chrono**: Date and time handling

---

## 9. Governance and Compliance

### 9.1 Development Model

- **Open Source**: Core platform under open source license
- **Community Input**: Feature decisions based on user feedback
- **Security Priority**: Immediate implementation of security updates
- **Backward Compatibility**: Careful API evolution with migration support

### 9.2 Compliance Considerations

- **Data Privacy**: GDPR/CCPA compliance design principles
- **Educational Privacy**: FERPA compliance for student records
- **Security Standards**: Implementation follows industry best practices
- **Audit Support**: Comprehensive logging for compliance verification

---

## 10. Conclusion

Hikmalayer provides a practical, secure solution for digital credential management through blockchain technology. By combining proof-of-work consensus, smart contract automation, and comprehensive APIs, the platform addresses real-world credentialing challenges while maintaining simplicity and performance.

The platform's focus on certificate management, combined with integrated token economics and developer-friendly architecture, creates immediate value for educational institutions and organizations while providing a foundation for future blockchain applications.

**Key Benefits:**

- **Tamper-proof credentials** through blockchain immutability
- **Instant verification** eliminating manual processes
- **Economic incentives** encouraging quality and participation
- **Easy integration** through comprehensive REST APIs
- **Scalable architecture** supporting growth and enhancement

**Target Applications:**

- Academic institutions issuing digital diplomas
- Professional organizations managing certifications
- Corporations tracking employee training and compliance
- Government agencies requiring secure document verification

Hikmalayer represents the next generation of credentialing systems, providing security, efficiency, and trust in an increasingly digital world.

---

## References

**Technical Resources:**

- Rust Documentation: https://doc.rust-lang.org/
- Axum Framework: https://docs.rs/axum/
- Blockchain Fundamentals: Nakamoto, S. "Bitcoin: A Peer-to-Peer Electronic Cash System"

**Standards and Compliance:**

- W3C Verifiable Credentials: https://www.w3.org/TR/vc-data-model/
- GDPR Compliance: EU 2016/679
- Educational Privacy: FERPA Guidelines

---

_This whitepaper is released under Creative Commons Attribution 4.0 International License. The information provided is for educational and informational purposes only._
