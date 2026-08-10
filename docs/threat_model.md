# Threat Model

**Status:** Current. Covers the system as implemented, including the
quantum-adversary case.

Companion documents: `security_assessment.md` (findings and fixes),
`quantum_readiness.md` (the hybrid scheme in detail), `wallet_security.md`
(key handling), `bridge_design.md` (why there is no bridge).

---

## 1. Assets

| Asset | Why an attacker wants it |
|---|---|
| Account private keys | Spend HKM and HTS balances, trade on the AMM |
| Validator staking keys | Unbond a validator's stake; produce blocks in its name |
| Validator VRF keys | Predict a validator's future slots |
| Issuer keys (Proof-of-Credential) | Forge or wrongly revoke credentials |
| Chain state and finalized blocks | Rewrite history; forge a balance or a credential |
| Admin / P2P bearer tokens | Drive the faucet, mining, difficulty and governance endpoints |
| Genesis parameters | Define a network that is not the network operators think they run |

---

## 2. Adversaries, and what stops them

### 2.1 An ordinary remote attacker

- **Forging a transaction from someone else's account.** The public key in a
  transaction must derive to the sender's address, and the signature must cover
  the exact canonical message — including the chain id, the operation prefix,
  and every field. Substituting any of it invalidates the signature.
- **Replaying a transaction.** Nonces are strictly sequential per account: no
  reuse, no skipping ahead to reserve a slot.
- **Replaying across networks.** The chain id is inside the signed bytes, so a
  transaction signed on a testnet is inert on mainnet even though the account
  exists on both. Enforced on the submission path *and* the block-validation
  path.
- **Re-encoding a transaction in flight.** Public keys have exactly one
  canonical spelling, so a relay cannot produce a second valid form of a
  transaction it is passing along.
- **Minting supply through arithmetic.** All balance and supply arithmetic is
  checked, and release builds keep overflow checks on.
- **Destroying funds via a mistyped recipient.** Malformed addresses are
  rejected rather than credited.

### 2.2 A malicious or faulty validator

- **Producing a block out of turn.** Selection is re-derived from the parent
  state and the VRF beacon; a block must come from the smallest open round that
  selects its producer.
- **Grinding randomness.** VRF outputs are unique per (key, slot); there is
  nothing to try.
- **Tampering with execution.** The state root commits to the state after the
  block; every node re-executes and compares.
- **Stalling the chain by going offline.** Slot timeouts open fallback leaders.
- **Provable misbehaviour** (wrong slot, bad signature, bad Proof-of-Work,
  tampered payload or state) is distinguished from structural problems and is
  slashable.

**Residual:** transaction *ordering within a block* is chosen by that block's
producer. That is true of every blockchain. Consequence: AMM trades must carry
slippage bounds — which the protocol requires as a field and the SDK refuses to
omit. Hikmalayer claims no front-running immunity.

### 2.3 A network attacker (DoS, eclipse)

- Gossip envelopes are signed by the sender's node key and bound to its derived
  node id; `P2P_REQUIRE_IDENTITY=true` rejects unsigned envelopes.
- Peer reputation scoring auto-bans repeat offenders; `P2P_ALLOWLIST` can
  restrict participation to named node ids.
- The mempool is capped, inputs are length-limited, and an inapplicable
  transaction costs one verification rather than a scan of the pool.

**Residual:** per-IP request rate limiting is **not** implemented. Deploy behind
a reverse proxy that provides it. Eclipse resistance depends on peer diversity,
which is an operational property, not a protocol guarantee.

### 2.4 A quantum adversary

**Assumption:** the adversary can run Shor's algorithm and therefore holds the
private key of any secp256k1 or sr25519 public key the chain has published.

This is not a distant hypothetical for key *exposure*: Hikmalayer publishes a
public key with **every** transaction, and a validator's key sits in the
on-chain staker set for as long as its stake does. "Harvest now, decrypt later"
means the archive is already being built.

| Target | Classical (`hkm…`) | Quantum-ready (`hkq…`) |
|---|---|---|
| Spend the balance | **Compromised** | Safe — also needs ML-DSA-65 |
| Unbond the stake | **Compromised** | Safe — verified against the on-chain ML-DSA key |
| Produce blocks in its name | **Compromised** | Safe — blocks need both signatures |
| Forge or revoke credentials | **Compromised** | Safe |
| Predict its VRF slots | **Compromised** | **Compromised** — the VRF is still classical |
| Break hashing, addresses, PoW | Safe (Grover only halves SHA-256) | Safe |
| Read stored wallet vaults | Safe (AES-256-GCM) | Safe |

**Downgrade attacks are the thing to get right**, and are closed explicitly:

- The **address** decides which scheme authorizes it, never the transaction, so
  omitting the post-quantum half is a rejection rather than a downgrade.
- A hybrid address commits to **both** public keys, so an attacker cannot pair
  the victim's classical key with an ML-DSA key of their own.
- The same rule holds on the stake path (against the key registered on chain)
  and the block path.
- A classical transaction carrying post-quantum fields is rejected too, so one
  authorization has exactly one encoding.

**Residual:**

- **The sr25519 VRF is classical.** Slot prediction is a real advantage for a
  targeted denial-of-service. It is not a path to forging blocks or spending,
  because block signatures are separately hybrid. No standardized post-quantum
  VRF exists yet.
- **Accounts that do not migrate stay exposed.** Migration is a transfer the
  user must make; no protocol can do it for them, and any mechanism that let a
  classical signature claim a hybrid account's funds would *be* the downgrade
  attack.
- **A classical break of ML-DSA** would not by itself compromise a hybrid
  account, since ECDSA is still required. This is why the scheme is hybrid.

### 2.5 An insider, or a compromised operator machine

- The node **never accepts a private key** on any endpoint.
- Admin and P2P endpoints are deny-by-default: an unset token disables them.
  Tokens support rotation and are compared in constant time.
- Wallet keys are AES-256-GCM at rest under PBKDF2-SHA256 (310,000 iterations);
  while unlocked they are held under a non-extractable WebCrypto key and
  decrypted only for the instant of signing.

**Residual:**

- **Admin tokens are bearer credentials, not signatures.** Anyone holding
  `ADMIN_TOKEN` can drive those endpoints. Treat it as a production secret.
- **Malware on the device defeats the wallet**, as it defeats any software
  wallet. Treasury and validator keys belong offline.
- **Genesis configuration is trust-critical.** Chain id, supply, validator
  allowlist, hybrid requirement and the genesis validator's keys are all
  committed to by the genesis state root — get them wrong and you have defined
  a different network. A hybrid genesis validator without its ML-DSA key is
  refused seating rather than seated weakly.

---

## 3. Explicitly out of scope

- **Independent audit.** None has been performed. Everything in
  `security_assessment.md` was found by the people who wrote the code.
- **Formal verification.** Not performed.
- **The browser extension** has not been published or externally reviewed. It
  is the component users trust with their keys.
- **Physical and supply-chain attacks** on operator hardware.
- **Governance capture.** A small validator set is cheap to outvote;
  `GENESIS_VALIDATOR_ALLOWLIST` gates who may join, and opening it is a
  governance decision with real security consequences.
