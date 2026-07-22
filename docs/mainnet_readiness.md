# Mainnet Readiness

This document tracks what the current codebase already enforces and what still
stands between this chain and a responsible **public mainnet** launch. It is
kept deliberately honest: a mainnet holds real value, so every open item below
is a launch blocker until closed.

## ✅ Implemented and tested

| Area | Status |
|---|---|
| Replicated state machine | Balances, validator set, nonces, and slashing are a deterministic function of the blocks; every block commits a **state root** re-verified by every node |
| On-chain validator set | Stake / withdraw are signed on-chain transactions; the validator set is derived from state, not node-local bookkeeping |
| Native identity | `hkm…` addresses (SHA-256 over secp256k1 pubkey) and a native signing domain — no external-chain conventions |
| Hybrid consensus | PoS stake-weighted selection from the on-chain set + PoW finalization |
| Unbiasable leader election | sr25519 VRF beacon (schnorrkel): every block carries a verifiable, ungrindable randomness contribution seeding the next slot (residual bias: withhold-and-forfeit only, the standard Praos/RANDAO bound) |
| Credential registry | Proof-of-Credential: issue/revoke as consensus objects, hash-only on-chain, state-root-bound proofs |
| Token hygiene | Rotating admin/P2P tokens (HM-06) with constant-time comparison (HM-07) |
| Block integrity | Deterministic genesis, Merkle root and state root both committed into the mined hash, full-chain replay |
| Validator authentication | Block signature must match the validator's on-chain registered key |
| Transaction security | All transfers / stakes / withdrawals carry native signatures, verified at ingress and re-verified + re-executed at consensus |
| Replay protection | Per-account on-chain nonces; bounded message-ID cache (P2P) |
| Fork choice | Cumulative-work comparison, full re-execution under local params, finalized-history protection |
| Sync | Peers serve `/p2p/chain`; nodes auto-sync on tip mismatch and rebuild state from genesis |
| Slashing | Permissionless equivocation proofs burn the offender's stake on-chain; double-slash prevented |
| Authorization | Deny-by-default admin and P2P tokens |
| Resource bounds | Difficulty clamped 1–5; input length limits |
| Rewards & fees | **Calibrated mainnet emission**: 6-decimal HKM, 20B genesis + ~80B mined, 5,000 HKM initial reward halving every 8,000,000 blocks (~3.8y), **50 HKM/block tail emission** as a perpetual security budget — all consensus-verified per height. Plus a **dynamic base fee** (EIP-1559-style, floor 0.001 HKM, cap 100 HKM) paid to the block validator; the base fee lives in the state root and is recomputed identically by every node |
| Vesting (lockups) | **On-chain cliff + linear vesting**: `Vest` transactions lock funds in a consensus-managed pool that releases block-by-block after the cliff; schedules are inspectable at `GET /vesting/{address}` — team/investor lockups are protocol guarantees, live-verified end-to-end |
| Validator floor | **10,000 HKM minimum stake** to join (or remain in, on withdrawal) the validator set — no trivial-stake spam validators |
| Sovereign finality | Fork choice is **validator-progress-first**: finalized blocks are irreversible, a fork must carry MORE validator-sealed blocks to displace the local chain, cumulative PoW only breaks exact ties, and fork tips future-dated beyond the clock-skew bound are rejected outright. Hashrate without stake produces nothing and reorgs nothing |
| Launch posture (permissioned hybrid) | Optional `GENESIS_VALIDATOR_ALLOWLIST` baked into the genesis state root: only listed addresses can register validator stakes at launch (existing validators top up freely); opened later via a scheduled upgrade. Documented honestly as permissioned-at-launch. PoW is self-mined by the selected leader with consensus-derived, clamped difficulty (1–5 hex zeros) and the 30s leader-rotation timeout — the chain never waits on external miners, and no separate miner infrastructure exists or is needed |
| Unbonding | Withdrawn stake stays locked and slashable for UNBONDING_BLOCKS before release; exit only completes when nothing remains bonded or unbonding |
| Slashing window | Equivocation proofs accepted only within SLASHING_WINDOW_BLOCKS (= unbonding period), so misbehaving stake can never exit before punishment |
| Difficulty retargeting | Deterministic per-chain schedule (every 10 blocks toward 15s target); block difficulty is consensus-validated, not operator-set |
| Liveness (leader rotation) | Slot-timeout fallback: the VRF-selected primary leader has 30s to produce; each timeout opens the next round's leader, so an offline validator delays the chain by at most one timeout — live-verified by killing a validator mid-network. Block timestamps are consensus-constrained in both directions (never before the parent, bounded future skew), closing retarget manipulation |
| Credential rotation (R-05) | Optional HMAC-signed self-expiring admin/P2P tokens minted offline (`mint_token`); stateless, scope-bound, constant-time, fail-closed verification alongside static token rotation |
| Atomic persistence (HM-05) | `save_state` writes to a temp file and renames — a crash mid-write can never corrupt the node's state file |
| Node responsiveness | PoW mining runs on the blocking thread pool with a tip-moved recheck; hot reads use an O(1) integrity probe |
| DoS bounds | Mempool cap (1,000 txs), per-block tx cap (100), 1 MiB request-body limit |
| P2P node identity | Every gossip envelope is signed by the sender's node key; `node_id` = derived address; `P2P_REQUIRE_IDENTITY=true` rejects unsigned envelopes (per-node keypair handshake atop the bearer token) |
| Peer reputation | Per-node scoring: useful blocks/txs raise reputation, invalid/malformed messages lower it, and repeat offenders are auto-banned; optional `P2P_ALLOWLIST` restricts participation to named validator node ids; `GET /p2p/peers/scores` |
| Snapshots & checkpoints | `GET /snapshot` exports the tip state + authenticating commitments (backup/inspection); `GET /checkpoint` returns a pinnable finalized (height, block_hash, state_root) triple for weak-subjectivity anchoring; trust-minimizing genesis replay remains the default |
| Checkpoint fast-sync / pruning | `GET /checkpoint/bundle` (p2p) serves a self-verifying bundle (retarget-boundary anchor + state + forward blocks); `HIKMALAYER_CHECKPOINT=<bundle.json>` boots a fresh node from the anchor without full genesis replay and reconstructs a byte-identical state root, randomness beacon, and difficulty; anchor pinned to a retarget boundary, state-root-bound, forward blocks re-validated; a persisted local chain always takes precedence |
| Observability | Metrics include blocks mined/received/rejected, reorgs, gossip, txs, slashes, peers banned, invalid-from-peers; structured startup logging of identity/enforcement |
| Tooling | `hikma-wallet` offline keygen/signing; propose/sign/submit flow for external validators |
| Tests | 97 automated tests across consensus, state machine, security, replay, fork choice (sovereign finality), slashing, emission (halving + tail), vesting, min-stake, validator allowlist, checkpoint fast-sync, leader rotation, token auth, and API flows |

## 🚧 Remaining before mainnet

The engineering surface is now built and tested. What remains is **design and
external process**, not missing protocol code:

1. **External security audit + adversarial testnet (the hard gate).**
   Independent audit of consensus and cryptography, plus a public incentivized
   testnet with adversarial validators, before any mainnet genesis. This
   *cannot* be self-performed — see the step-by-step
   [`docs/external_audit_guide.md`](external_audit_guide.md).

2. **Emission/treasury policy.** ✅ *Calibrated.* The mainnet parameter set is
   implemented and consensus-verified: 6-decimal HKM, 20B genesis / ~80B mined
   toward a ~100B supply at tail start, 8M-block halvings, a 50 HKM/block tail
   emission for the long-run security budget, a 10,000 HKM validator floor,
   and on-chain vesting for allocations. What remains is *distribution policy*
   (who receives what from the 20B genesis treasury, published as on-chain
   vesting schedules at launch) — a business decision, not code.

3. **Production key management.** `VALIDATOR_PRIVATE_KEY` via environment
   variable is fine for testnets; production validators should use an HSM,
   OS keyring, or remote signer (see `docs/key_management.md`). This is an
   operator deployment choice; the node already never handles foreign keys.

4. **State pruning for very long chains.** ✅ *Implemented.* Checkpoint
   fast-sync now exists: `GET /checkpoint/bundle` serves a self-verifying,
   retarget-boundary-anchored bundle, and `HIKMALAYER_CHECKPOINT` boots a fresh
   node from it without full genesis replay — reconstructing a byte-identical
   state root, randomness beacon, and difficulty (verified live and in an
   automated equivalence test). Full trust-minimizing replay remains the
   default; fast-sync is an explicit, opt-in weak-subjectivity assumption for
   long-lived deployments. What remains is operational: publishing and pinning
   community-agreed checkpoint anchors.

## Suggested order of work

`economic modeling` → `deploy adversarial testnet` → `external audit + fixes`
→ `mainnet genesis`. Scaling (checkpoint fast-sync / pruning) is now built.
