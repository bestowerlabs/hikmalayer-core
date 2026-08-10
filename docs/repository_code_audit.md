# Repository Code Audit

An assessment from the **code**, not from claims made about it. Refreshed to
cover the current tree.

## Questions addressed

1. Is this a working hybrid blockchain on both PoS and PoW?
2. Is it quantum-ready?
3. Is it production-grade?

## Verdict

| | |
|---|---|
| **Hybrid PoS + PoW implemented in code** | **Yes** |
| **Quantum-ready hybrid signatures implemented in code** | **Yes** — and enforced on every path where a key authorizes value or consensus |
| **Production-grade / mainnet-ready** | **Not yet** — one hard gate remains: independent external audit |

---

## 1. Hybrid consensus — implemented

- Stake-weighted leader selection over the on-chain validator set at the
  **parent** state (`pos::select_staker_with_seed`), seeded by the VRF beacon.
- sr25519 VRF proof required per block, verified against the key registered on
  chain (`consensus::vrf`).
- Proof-of-Work mined by the selected leader, difficulty deterministic and
  clamped (`consensus::pow`).
- Liveness rotation via slot timeouts, with a block required to come from the
  smallest open round that selects its producer.
- Fork choice is validator-progress-first with finalized-history protection;
  adopted chains are re-executed under local genesis parameters.
- Equivocation slashing, bounded by a slashing window equal to the unbonding
  period.

**Where to read it:** `src/blockchain/chain.rs::validate_block_at`,
`src/consensus/{pos,vrf,pow}.rs`.

## 2. Dual-hybrid cryptography — implemented

- ML-DSA-65 (FIPS 204) via `fips204`, with deterministic keygen and signing
  (`src/consensus/pq.rs`).
- Hybrid identity where the **address commits to both public keys**
  (`src/consensus/hybrid.rs`), so neither can be substituted.
- The **address**, not the transaction, decides which scheme authorizes it
  (`transaction.rs::verify_sender_signature`) — so a hybrid account cannot be
  downgraded by omitting the post-quantum half, and a classical transaction
  carrying post-quantum fields is rejected.
- Enforced on staking, **unbonding** (against the on-chain key) and **block
  production**, not only on transfers.
- Deterministic across implementations: the Rust node and the JavaScript
  clients produce byte-identical keys and signatures, asserted by test.

**Where to read it:** `src/consensus/{pq,hybrid}.rs`, `sdk/src/hybrid.js`,
`dashboard/src/lib/hybrid.js`, `sdk/test/hybrid.test.mjs`.

## 3. State machine and economics — implemented

- Replicated state machine with a per-block **state root**, verified by
  re-execution.
- Checked arithmetic throughout, `overflow-checks` on in release.
- HTS token standard with fixed supply at creation; constant-product AMM with
  `MINIMUM_LIQUIDITY` lock and mandatory slippage bounds; on-chain cliff and
  linear vesting; Proof-of-Credential registry.
- Calibrated emission (halving plus perpetual tail) and a dynamic base fee, both
  consensus-verified per height.

**Where to read it:** `src/blockchain/state.rs`, `src/blockchain/transaction.rs`.

## 4. What is genuinely not there

- **No virtual machine.** Deliberate — `ContractExecutor` is a credential
  registry, not an execution environment. Anyone reading it as a smart-contract
  engine is misreading it.
- **No bridge.** No code exists and none is planned.
- **No per-IP rate limiting.** Deploy behind a proxy.
- **No post-quantum VRF.** Leader election remains classical.
- **No multi-signature or threshold accounts.**
- **No seed phrase / HD derivation.**

## 5. What separates this from production-grade

Not missing protocol code. Specifically:

1. **No independent audit.** The 13 findings in `security_assessment.md` were
   found by the people who wrote the code. That is the largest open risk in the
   repository.
2. **No public adversarial testnet.** All measurement to date is single-host;
   wide-area consensus behaviour is unmeasured.
3. **Permissioned launch posture.** A genesis allowlist may gate validator
   registration.
4. **Operational gaps.** No bug bounty, security contact process, CI dependency
   scanning or fuzzing.

## 6. Evidence a reader can run

```bash
cargo test                             # 139 unit tests
cargo test --release --test security   # 40 adversarial tests
cargo clippy --all-targets -- -D warnings
cd sdk && npm test                     # 58 offline, incl. Rust↔JS byte parity
```

`tests/security.rs` is the most informative single file for an auditor: each
test plays an attacker with a specific goal and asserts refusal, and the quantum
tests assume the attacker already holds the victim's secp256k1 private key.
