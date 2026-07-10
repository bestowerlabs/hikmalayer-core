# External Security Audit — Step-by-Step Guide

This document is written **for an independent external auditor** engaged to
review Hikmalayer Core before any public mainnet. It tells the auditor exactly
how to reproduce the build, what to review, in what order, and what to deliver.
It also tells the Hikmalayer team how to prepare for and respond to the audit.

An external audit is a **launch blocker**: no real value should touch this
chain until a reputable third party has completed the steps below and their
findings have been remediated and re-checked.

---

## 0. What "external" means (read first)

- The auditor must be an **independent third party** — a firm or individuals
  with no stake in the project, no tokens, and no development role. Self-audits
  and audits by the founding team do **not** satisfy this requirement.
- Reputable options for an L1 of this kind: Trail of Bits, NCC Group, Least
  Authority, Sigma Prime, Quantstamp, OtterSec, Zellic, Halborn, or an
  academic cryptography group for the VRF/consensus portions.
- Scope should cover **consensus, cryptography, state machine, P2P, and node
  operations** — not just "the smart contracts" (this chain has none in the
  EVM sense; the risk lives in the protocol).

---

## 1. Engagement setup (team + auditor)

1. **Freeze a commit.** The team tags an immutable audit target, e.g.
   `git tag audit-2026-q3 <commit> && git push origin audit-2026-q3`. The
   auditor works against that tag only; mid-audit changes restart the clock.
2. **Scope document.** Agree in writing on: in-scope paths (see §3), the
   threat model (`docs/threat_model.md`), severity definitions (§7), the
   report format, and the re-audit terms for fixes.
3. **Provide artifacts.** Hand the auditor: this repo at the tag, the
   whitepaper (`docs/Whitepaper.md`), `docs/consensus_flow.md`,
   `docs/mainnet_readiness.md`, `docs/threat_model.md`, and the test suite.
4. **Access.** Read access to the repo, CI logs, and a dedicated audit
   testnet the team stands up (see §6). The auditor should **not** need any
   production secrets.
5. **Point of contact.** Name one engineer who answers questions within one
   business day. Keep a shared question log.

---

## 2. Reproduce the build and tests (auditor, day 1)

The audit is worthless if the auditor can't build the exact target. Verify a
clean, deterministic build first:

```bash
git clone <repo> hikmalayer && cd hikmalayer
git checkout audit-2026-q3
git rev-parse HEAD                 # record this hash in the report

# Toolchain: pin and record the versions used
rustc --version && cargo --version

# Reproducible build + full test suite (must be green)
cargo build --release --all-targets
cargo test                          # expect: all tests pass, 0 failures

# Warnings and lints
cargo build --release 2>&1 | grep -i warning   # expect: none
cargo clippy --all-targets -- -D warnings       # run and record findings
cargo fmt --check                               # style drift

# Dependency review
cargo tree > deps.txt
cargo audit                          # RUSTSEC advisories (install cargo-audit)
cargo deny check                     # licenses + bans + advisories (optional)
```

Deliverable for this step: a short "reproduction" note confirming the commit
hash, toolchain versions, that tests pass, and any clippy/`cargo audit`
findings.

---

## 3. Code review map (what to read, in priority order)

Review is prioritized by blast radius. For each item the auditor should
confirm the property in the "Check" column by reading the code **and** writing
an adversarial test that tries to violate it.

| # | Area | Files | Check |
|---|------|-------|-------|
| 1 | **State machine** | `src/blockchain/state.rs` | Every mutation is deterministic; `state_root()` is canonical (BTreeMap ordering); no path mints value except `Reward`; fees conserve supply; unbonding math and slash deduction cannot underflow or double-spend |
| 2 | **Block validation** | `src/blockchain/chain.rs` (`validate_block_at`, `validate_full`, `try_adopt_chain`) | A block is accepted **iff** index/prev-hash link, PoS slot, on-chain signer key, VRF proof, Merkle root, PoW, difficulty schedule, all tx signatures, exactly-one-reward, timestamp skew, and re-executed `state_root` all hold. Fork choice re-executes under **local** genesis params and never rewrites finalized history |
| 3 | **Consensus crypto** | `src/consensus/pos.rs`, `src/consensus/vrf.rs`, `src/consensus/pow.rs` | secp256k1 sign/verify used correctly; address derivation is collision-resistant; VRF (schnorrkel) proofs are unique per (key, slot) and verified against the registered key; difficulty bounds prevent PoW-disable and unbounded mining |
| 4 | **Transactions** | `src/blockchain/transaction.rs` | Signing messages are unambiguous and domain-separated; `verify_for_block` covers every type; the equivocation `SlashProof` is unforgeable (requires the victim's own signatures on two distinct same-height blocks) |
| 5 | **P2P** | `src/p2p/*`, envelope handling in `src/api/routes.rs` | Envelope identity signatures bind node_id; replay cache bounds memory; gossip cannot be used to flood or to inject invalid state; `require_identity` actually rejects unsigned envelopes |
| 6 | **API / auth** | `src/api/routes.rs`, `src/auth/*` | Deny-by-default admin/P2P tokens; constant-time comparison; every mutating endpoint validates + re-executes; mempool/body caps enforced; no endpoint accepts a private key |
| 7 | **Persistence / bootstrap** | `src/persistence.rs`, `src/main.rs` | Only the chain is persisted; state is rebuilt by replay and rejected if it fails; genesis parameters are part of chain identity; dev keys are clearly dev-only |

---

## 4. Property & adversarial testing (auditor)

Beyond reading, the auditor should try to **break** the invariants. Concrete
attacks to attempt (each should fail):

**Consensus / economics**
1. Produce a block for a validator PoS did *not* select → must be rejected.
2. Sign a block with a key not registered on-chain for that validator → reject.
3. Forge a `state_root` that grants free balance → reject ("state root").
4. Include two reward txs, or a reward to a non-validator, or an inflated
   reward → reject.
5. Grind block contents to bias the next leader → VRF makes the output unique;
   confirm no grinding advantage beyond withhold-and-forfeit.
6. Submit a block whose difficulty differs from the retarget schedule → reject.
7. Double-spend by replaying a signed transfer (same nonce) → reject.
8. Overspend including the fee (`balance == amount`, fee unpayable) → reject.

**Slashing / exit safety**
9. Equivocate, then withdraw and try to exit before the slash → unbonding +
   slashing-window must keep the stake slashable; verify the burn still lands.
10. Submit an equivocation proof after the window → reject.
11. Double-slash the same offense → reject.
12. Forge an equivocation proof against an honest validator → impossible
    without that validator's signatures; confirm.

**P2P / DoS**
13. Replay a gossip envelope → reject (message-ID cache).
14. Send an unsigned/mis-signed envelope with `P2P_REQUIRE_IDENTITY=true` →
    reject.
15. Flood the mempool past the cap, or POST a body over 1 MiB → bounded.
16. Feed a peer a longer-but-invalid chain → fork choice must re-execute and
    reject; finalized history must never be rewritten.

**Determinism**
17. Replay the same block history on two machines/architectures → identical
    `state_root` and beacon (fuzz transaction ordering within the nonce rules).

Recommended tooling: `cargo test`, `proptest`/`quickcheck` for state-machine
properties, `cargo fuzz` (libFuzzer) targeting `Transaction`/`Block`/envelope
deserialization and `validate_full`, and a small multi-node harness driving
the REST API (see `ops/start_testnet.sh`).

---

## 5. Cryptography deep-dive (auditor, specialist)

- Confirm `secp256k1` and `schnorrkel` are used with audited defaults; no
  home-rolled primitives.
- Verify domain separation: identity signing prefix
  (`consensus::pos::MESSAGE_PREFIX`), VRF context (`consensus::vrf::VRF_CONTEXT`),
  and each transaction/credential signing message are mutually unambiguous —
  a signature for one purpose must not validate for another.
- Assess the randomness beacon: is the withhold-and-forfeit bias acceptable
  for the target validator count and reward? Recommend parameters or a
  commit-reveal augmentation if not.
- Review key management guidance (`docs/key_management.md`) against the HSM /
  remote-signer requirement for production validators.

---

## 6. Adversarial testnet (team + auditor, in parallel)

Code review alone is insufficient for a consensus system. Run a public or
consortium **incentivized testnet with adversarial validators** for several
weeks:

1. Stand up ≥ 4 validators across independent operators/hosts with real
   genesis parameters (not the dev keys), `P2P_REQUIRE_IDENTITY=true`, and
   monitoring (Prometheus/Grafana are already wired in `docker-compose.yml`).
2. Invite participants to attack: equivocate, censor, grind, spam, partition
   the network, submit malformed blocks/txs, and attempt reorgs.
3. Offer bounties for any consensus fault, state divergence between honest
   nodes, unslashed equivocation, minted-from-nothing value, or liveness
   failure.
4. Track: state-root agreement across honest nodes, reorg depth, slashing
   events, block-time vs. the 15s target (retargeting behavior), and memory
   over a multi-week run.

Exit criterion: a sustained run with no consensus faults and every injected
equivocation correctly slashed.

---

## 7. Severity classification (agree up front)

| Severity | Definition | Examples |
|---|---|---|
| **Critical** | Loss/mint of funds, chain halt, or forgery of consensus | Mint-from-nothing, forge a block/credential, unslashable equivocation, state divergence between honest nodes |
| **High** | Exploitable but bounded, or requires a condition | Griefing that can halt a validator, replay under a rare race, unbounded resource use |
| **Medium** | Security-relevant weakness, hard to exploit | Weak parameter defaults, missing rate limit, info leak |
| **Low / Informational** | Best-practice, hardening, docs | Lint findings, unclear error paths, dependency freshness |

---

## 8. Deliverables (auditor)

1. **Draft report**: every finding with severity, affected file/line at the
   audited commit, a concrete reproduction (ideally a failing test or PoC),
   impact, and a recommended fix.
2. **Reproduction note** from §2 (commit hash, toolchain, test result).
3. **Fix-review round**: after the team remediates, the auditor re-checks each
   finding against the new commit and marks it fixed / partially fixed / not
   fixed.
4. **Final report** cleared for publication, plus a short attestation
   (scope, commit, dates, residual risks).

---

## 9. Remediation & disclosure (team)

1. Fix Critical/High before any mainnet; re-audit the fixes (§8.3).
2. Keep a `SECURITY.md` with a private disclosure address and a bug-bounty
   policy live **before** launch.
3. Publish the final report and the remediated commit hash. Record the audit
   in `docs/audit_readiness_pack.md`.
4. Re-audit after any consensus-affecting change post-launch.

---

## 10. Quick reference — one-command reproduction

```bash
git checkout audit-2026-q3 && \
cargo build --release --all-targets && \
cargo test && \
cargo clippy --all-targets -- -D warnings && \
cargo audit
```

If any of these fail, stop and resolve with the team before proceeding — the
audit target must be clean and reproducible.
