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
| Rewards | Fixed block reward, consensus-verified (recipient + amount + exactly one per block) |
| Tooling | `hikma-wallet` offline keygen/signing; propose/sign/submit flow for external validators |
| Tests | 59 automated tests across consensus, state machine, security, replay, fork choice, slashing, and API flows |

## 🚧 Launch blockers (Phase 8)

1. **Signed peer handshakes / validator networking.** P2P is currently
   authenticated by a shared bearer token, suitable for a permissioned
   testnet. Mainnet requires per-node keypairs, signed handshakes, and
   peer scoring/banning.

2. **Mempool and block-size limits.** Pending-pool size, per-block
   transaction count, and per-request body limits need explicit caps and
   fee-based prioritization (fee market not yet designed). Currently one
   pending transaction per account per block (nonce must be the next value).

3. **Difficulty adjustment.** Difficulty is an operator-set constant. A
   retargeting algorithm tied to observed block times is needed. PoW is also
   a synchronous single-thread loop — production needs an async miner.

4. **Economic design.** Fixed 5-token block reward, no transaction fees, no
   halving schedule, no treasury policy. Token economics must be specified
   before value is attached.

5. **Unbonding period for withdrawals.** Stake can currently be withdrawn in
   the next block. Mainnet needs an unbonding delay so a validator cannot
   equivocate and immediately exit before a slash lands.

6. **Historical slashing window.** Equivocation proofs are accepted at any
   time; they should be bounded to an unbonding window and the offending
   blocks should be checkable against a retained header history.

7. **Key management hardening.** `VALIDATOR_PRIVATE_KEY` via environment
   variable is fine for testnets; production validators should use an HSM,
   OS keyring, or remote signer (see `docs/key_management.md`).

8. **State growth & snapshots.** State is held in memory and replayed from
   full block history on startup. Mainnet needs state snapshots / checkpoint
   sync and pruning so startup and memory do not grow unbounded.

9. **External security audit + adversarial testnet.** Independent audit of
    consensus and cryptography, plus a public incentivized testnet with
    adversarial validators, before any mainnet genesis.

10. **Observability & operations.** Structured logging, alerting on reorgs
    and validation failures, snapshot/restore tooling, and documented
    incident-response runbooks.

## Suggested order of work

`unbonding + slashing window` → `difficulty retarget + async miner` →
`mempool/fees` → `P2P identity` → `snapshots/pruning` → `economics` →
`audit + adversarial testnet`.
