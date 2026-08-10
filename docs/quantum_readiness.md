# Hikmalayer — Quantum Readiness

**Status:** implemented and enforced. Hybrid accounts are available on every
network, opt-in per account, and a network can require them at genesis.

---

## 1. The problem, stated honestly

Hikmalayer's original cryptography is:

| Where | Primitive | Quantum status |
|---|---|---|
| Account signatures | secp256k1 ECDSA | **Broken by Shor's algorithm** |
| Block signatures | secp256k1 ECDSA | **Broken by Shor's algorithm** |
| Leader election | sr25519 VRF (Ristretto255) | **Broken by Shor's algorithm** |
| Hashing, addresses, Merkle, PoW | SHA-256 | Sound — Grover halves it, 128-bit is still out of reach |
| Node transport | AES-256-GCM | Sound — 128-bit post-Grover |

So the hashing is fine and the signatures are not. That is the ordinary
position for every chain built on elliptic curves.

**Hikmalayer's exposure is worse than Bitcoin's, and it is worth being blunt
about why.** Bitcoin's P2PKH addresses publish only a hash of the public key;
the key itself appears when a coin is first spent, so an unspent, never-spent
output is not exposed. Hikmalayer publishes the public key with *every*
transaction, because verification takes it from the request. And a validator's
key sits in `StakeInfo` in the open for as long as its stake does — there is no
"spend once and rotate" for it. The longest-lived, most valuable key on the
chain is also the most exposed one.

"Harvest now, decrypt later" applies directly: an adversary can archive the
chain today and derive private keys the day the hardware exists.

---

## 2. What was built

A **hybrid** scheme: classical *and* post-quantum, both required.

```
hkm…   classical    address = SHA256(secp_pub)[..20]
                    authorized by:  ECDSA

hkq…   quantum-ready  address = SHA256(domain ‖ secp_pub ‖ mldsa_pub)[..20]
                    authorized by:  ECDSA  AND  ML-DSA-65
```

An attacker must break **both** schemes to forge one transaction. The account
stays safe as long as **either** holds.

That is not hedging for its own sake. Nobody can honestly say when a
cryptographically relevant quantum computer arrives — and equally, ML-DSA is
young enough that a classical break of it would not be shocking. Requiring both
means neither surprise is fatal. It is also why replacing secp256k1 outright
would have been the *worse* choice.

### The parameter set

**ML-DSA-65** (FIPS 204, NIST security category 3), via the pure-Rust
`fips204` crate on the node and `@noble/post-quantum` in JavaScript.

ML-DSA-44 is category 2 — a thinner margin than a chain holding value for
decades should accept. ML-DSA-87 is category 5 and costs another ~1.3 KB per
signature to defend against an adversary nobody can presently describe. 65 is
the balance, and it is what most deploying systems have chosen.

### The address commits to both keys

This is the part that is easy to get wrong, and getting it wrong makes the
whole exercise decorative.

If a hybrid address were derived from the classical key alone, an attacker who
broke secp256k1 could present the victim's classical key alongside an ML-DSA
key **of their own** — and the "hybrid" account would fall to a single break.
Hashing both keys into the address means substituting either one names a
different account. `tests/security.rs::substituting_a_post_quantum_key_does_not_reach_the_victims_account`
is that attack, executed, and refused.

### Determinism

Both key generation and signing are deterministic:

* **Keys** from a 32-byte seed ξ — FIPS 204's own keygen input (Algorithm 1) —
  derived from the account's existing private key with a domain separator. One
  backup still covers both schemes; there is no second secret to lose.
* **Signatures** from a seed derived from (key, message) rather than fresh
  randomness. This is the hedged variant with `rnd` pinned: standards-
  conforming, reproducible across implementations, and it means a broken RNG on
  a user's machine cannot leak a key through a signature.

Determinism is also what makes the Rust node and the browser wallet
interoperable. `sdk/test/hybrid.test.mjs` asserts byte-identical output against
the `hikma-wallet` CLI for both key derivation and signatures.

---

## 3. What is protected

| Surface | Classical account | Hybrid account |
|---|---|---|
| Transfers, tokens, vesting, DEX | ECDSA | ECDSA **+ ML-DSA-65** |
| Staking (bonding) | ECDSA | ECDSA **+ ML-DSA-65**, and the ML-DSA key is registered on chain |
| Unbonding stake | ECDSA vs. the on-chain key | **Both**, vs. the on-chain keys |
| Block production | ECDSA over the block hash | **Both** over the block hash |

The validator paths matter as much as the account paths. A hybrid account whose
*balance* needed two signatures but whose *stake* needed one would hand an
attacker with a broken secp256k1 the entire bonded amount —
`a_broken_secp256k1_alone_cannot_unbond_a_hybrid_validators_stake` pins that
shut. Likewise a hybrid validator's blocks carry a second signature, checked
against the ML-DSA key registered with the stake, because a validator's key is
the longest-lived key on the chain.

In every case **the address decides**, never the transaction. Reading the
scheme off the request would let an attacker downgrade a hybrid account by
simply omitting the post-quantum half.

---

## 4. What is *not* protected, and why

Stated plainly rather than buried:

* **The VRF (sr25519) is still classical.** Leader election uses it, and a
  quantum adversary that recovers a validator's VRF key could predict — not
  forge — that validator's slots in advance. It cannot mint, spend, or produce
  a block another validator's key would be needed for, because the block
  signature is separately hybrid. There is no standardized post-quantum VRF
  today; the honest options are a hash-based construction with weaker
  unpredictability properties, or waiting. This is the one place where the
  current answer is "classical, deliberately, and documented."
* **Classical accounts stay classical.** `hkm…` accounts behave exactly as
  before. Hybrid is opt-in per account, because forcing a 40× signature-size
  increase on every user from day one is not a defensible default.
* **Migration is a transfer, not an upgrade.** A key's classical and hybrid
  accounts are *different accounts with separate balances*. Moving to hybrid
  means sending funds from one to the other. There is no in-place conversion,
  and there is deliberately no mechanism by which a classical signature can
  claim a hybrid account's funds — such a mechanism would be the downgrade
  attack, offered as a feature.
* **Nothing here helps if the key is stolen conventionally.** Malware on the
  device, a phished seed, an unlocked wallet — hybrid changes none of that.

---

## 5. The cost

| | Classical | Hybrid |
|---|---|---|
| Public key | 65 bytes | 65 + 1,952 = 2,017 bytes |
| Signature | 64 bytes | 64 + 3,309 = 3,373 bytes |
| Per transaction, on the wire | ~130 bytes | ~5.4 KB (~40×) |
| Signing time (browser) | < 1 ms | ~11 ms |
| Key derivation | < 1 ms | ~3 ms |

This is the real, unavoidable price of post-quantum signatures today. It is why
hybrid is opt-in per account rather than mandatory, and why a network that
wants it universally must decide so at genesis with its eyes open.

---

## 6. Using it

### CLI

```bash
# One secret, two identities — the hybrid one is derived, not separate.
hikma-wallet keygen
hikma-wallet identity <private_key_hex>

# Sign for the hkq account: both signatures are emitted.
HIKMALAYER_HYBRID=1 hikma-wallet sign-transfer <hkq_from> <to> <amount> <nonce> <key>

# A hybrid validator signs blocks under both schemes too.
HIKMALAYER_HYBRID=1 hikma-wallet sign-block <block_hash> <key>
```

### SDK

```js
import { HikmalayerClient, HybridSigner } from "@hikmalayer/sdk";

const client = HikmalayerClient.withHybridPrivateKey(process.env.KEY, {
  url: "http://127.0.0.1:3000",
});

// Identical API. Every write now carries pq_public_key and pq_signature.
await client.transfer({ to: "hkq…", amount: parseUnits("1.5") });
```

### Browser wallet and extension

Both derive the hybrid identity on unlock and expose a **Classical /
Quantum-ready** switch. Selecting quantum-ready changes which account the UI —
and any connected site — is operating as; connected sites are notified as they
would be for any account change.

### Running a network that requires it

```bash
GENESIS_REQUIRE_HYBRID=1 ./hikmalayer
```

The flag is baked into the genesis state root, so it is a property of the
network rather than a local setting nodes could quietly differ on. Every sender
must then be a `hkq…` account.

---

## 7. Verification

```bash
cargo test                          # 138 unit tests
cargo test --release --test security   # 36 adversarial tests
cd sdk && npm test                  # 57 offline, incl. Rust↔JS parity
ops/devnet.sh &                     # live chain
cd sdk && HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
```

The tests that specifically establish the quantum claims:

| Test | What it proves |
|---|---|
| `a_broken_secp256k1_alone_cannot_spend_a_hybrid_account` | A genuine ECDSA signature — everything a broken secp256k1 gives an attacker — does not move a hybrid account |
| `substituting_a_post_quantum_key_does_not_reach_the_victims_account` | Pairing the victim's classical key with the attacker's ML-DSA key names a different account |
| `a_broken_ml_dsa_alone_cannot_spend_a_hybrid_account` | The converse: the account survives an ML-DSA break too |
| `a_broken_secp256k1_alone_cannot_unbond_a_hybrid_validators_stake` | Stake is protected as well as balance |
| `a_block_from_a_hybrid_validator_without_the_post_quantum_signature_is_rejected` | Block production cannot be downgraded |
| `a_classical_transaction_carrying_post_quantum_fields_is_refused` | One authorized transaction has exactly one valid encoding |
| `the_two_signatures_must_cover_the_same_message` | Signing different messages under the two schemes authorizes neither |
| `a_hybrid_signature_does_not_replay_across_networks` | Network scoping covers the post-quantum half |
| `sdk/test/hybrid.test.mjs` parity cases | The browser and the node produce identical bytes |
