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
| Rewards & fees | Fixed block reward + flat per-tx fee paid to the block validator, all consensus-verified |
| Unbonding | Withdrawn stake stays locked and slashable for UNBONDING_BLOCKS before release; exit only completes when nothing remains bonded or unbonding |
| Slashing window | Equivocation proofs accepted only within SLASHING_WINDOW_BLOCKS (= unbonding period), so misbehaving stake can never exit before punishment |
| Difficulty retargeting | Deterministic per-chain schedule (every 10 blocks toward 15s target); block difficulty is consensus-validated, not operator-set |
| Node responsiveness | PoW mining runs on the blocking thread pool with a tip-moved recheck; hot reads use an O(1) integrity probe |
| DoS bounds | Mempool cap (1,000 txs), per-block tx cap (100), 1 MiB request-body limit |
| P2P node identity | Every gossip envelope is signed by the sender's node key; `node_id` = derived address; `P2P_REQUIRE_IDENTITY=true` rejects unsigned envelopes (per-node keypair handshake atop the bearer token) |
| Peer reputation | Per-node scoring: useful blocks/txs raise reputation, invalid/malformed messages lower it, and repeat offenders are auto-banned; optional `P2P_ALLOWLIST` restricts participation to named validator node ids; `GET /p2p/peers/scores` |
| Snapshots & checkpoints | `GET /snapshot` exports the tip state + authenticating commitments (backup/inspection); `GET /checkpoint` returns a pinnable finalized (height, block_hash, state_root) triple for weak-subjectivity anchoring; trust-minimizing genesis replay remains the default |
| Observability | Metrics include blocks mined/received/rejected, reorgs, gossip, txs, slashes, peers banned, invalid-from-peers; structured startup logging of identity/enforcement |
| Tooling | `hikma-wallet` offline keygen/signing; propose/sign/submit flow for external validators |
| Tests | 70 automated tests across consensus, state machine, security, replay, fork choice, slashing, and API flows |

## 🚧 Remaining before mainnet

The engineering surface is now built and tested. What remains is **design and
external process**, not missing protocol code:

1. **External security audit + adversarial testnet (the hard gate).**
   Independent audit of consensus and cryptography, plus a public incentivized
   testnet with adversarial validators, before any mainnet genesis. This
   *cannot* be self-performed — see the step-by-step
   [`docs/external_audit_guide.md`](external_audit_guide.md).

2. **Economic design (fee market + emission).** A flat per-tx fee and fixed
   block reward exist and are consensus-verified. A dynamic fee market
   (priority pricing, congestion response) and a long-term emission/treasury
   policy require token-economic modeling, not just code, before value is
   attached.

3. **Production key management.** `VALIDATOR_PRIVATE_KEY` via environment
   variable is fine for testnets; production validators should use an HSM,
   OS keyring, or remote signer (see `docs/key_management.md`). This is an
   operator deployment choice; the node already never handles foreign keys.

4. **State pruning for very long chains.** Snapshots and pinnable checkpoints
   now exist; full trust-minimizing replay remains the default. History
   pruning / checkpoint-based fast sync (skipping pre-checkpoint replay under
   an explicit weak-subjectivity assumption) is an optional scaling step for
   long-lived deployments.

## Suggested order of work

`economic modeling` → `deploy adversarial testnet` → `external audit + fixes`
→ `mainnet genesis`. Optional scaling: `checkpoint fast-sync / pruning`.
