# Audit Readiness Pack

What an external reviewer needs, and where it is. **No independent audit has
been performed** — everything in this repository was found by the people who
wrote it. This pack exists to make an external review cheap to start, not to
substitute for one.

## Start here

| Document | What it covers |
|---|---|
| [`security_assessment.md`](security_assessment.md) | 13 findings, all fixed, each with a regression test that fails on the pre-fix code. Read this first — it is the honest record of what went wrong |
| [`threat_model.md`](threat_model.md) | Adversaries, assets, mitigations, and what is explicitly out of scope — including the quantum adversary |
| [`quantum_readiness.md`](quantum_readiness.md) | The dual-hybrid signature scheme in full, including what it does **not** cover |
| [`Whitepaper.md`](Whitepaper.md) §7 | Cryptographic architecture as implemented |
| [`consensus_flow.md`](consensus_flow.md) | Block lifecycle |
| [`key_management.md`](key_management.md) | Every key, where it lives, rotation and loss |
| [`validator_lifecycle.md`](validator_lifecycle.md) | Join, produce, misbehave, exit |
| [`wallet_security.md`](wallet_security.md) | Browser wallet and extension security model |
| [`bridge_design.md`](bridge_design.md) | Why there is no bridge |
| [`mainnet_readiness.md`](mainnet_readiness.md) | What is built, what remains, what blocks launch |
| [`openapi.yaml`](openapi.yaml) | OpenAPI 3.1, lints clean |

## Highest-value review targets

Ranked by what a bug there would cost:

1. **`src/blockchain/state.rs`** — the state machine. Every balance, supply and
   AMM computation. Finding 1 (a supply-minting integer overflow) lived here.
2. **`src/consensus/hybrid.rs` and `src/consensus/pq.rs`** — the dual-hybrid
   scheme. In particular: the address must commit to both keys, and the
   *address* must decide which scheme authorizes it.
3. **`src/blockchain/transaction.rs::verify_sender_signature`** — the single
   authorization choke point.
4. **`src/blockchain/chain.rs::validate_block_at`** — selection, VRF, PoW, both
   signatures, state root.
5. **`src/consensus/pos.rs::canonical_public_key`** — encoding canonicality;
   finding 10 was a malleability bug closed here.

## Reproducing everything

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

`tests/security.rs` is the right entry point for an adversarial reviewer: each
test plays an attacker with a specific goal — mint supply, spend someone else's
funds, replay a signature, drain a pool, downgrade a hybrid account — and
asserts the chain refuses. The quantum tests assume the attacker **already
holds the victim's secp256k1 private key**.

## Known gaps a reviewer should not have to discover

- No independent audit; no formal verification.
- The sr25519 VRF is classical (see `quantum_readiness.md` §4).
- The browser extension has not been published or externally reviewed.
- Admin endpoints are token-gated, not signature-gated.
- Per-IP rate limiting is not implemented; deploy behind a proxy that provides it.
- Permissioned launch posture via `GENESIS_VALIDATOR_ALLOWLIST`.
- No seed phrase / HD derivation — each account key needs its own backup.
