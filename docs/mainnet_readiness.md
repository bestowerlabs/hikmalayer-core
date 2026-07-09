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
| Tooling | `hikma-wallet` offline keygen/signing; propose/sign/submit flow for external validators |
| Tests | 65 automated tests across consensus, state machine, security, replay, fork choice, slashing, and API flows |

## 🚧 Launch blockers (Phase 9)

1. **Peer scoring / banning.** Envelopes are now signed by per-node keys
   (`P2P_REQUIRE_IDENTITY`), so peers are cryptographically identified.
   Reputation scoring, misbehavior banning, and an allow-list of permitted
   validator node keys still need to be layered on top.

2. **Fee-market refinement.** A flat per-tx fee exists; a dynamic fee market
   (priority pricing, congestion response) and long-term emission policy
   (halving/treasury) still need economic design before value is attached.

3. **Key management hardening.** `VALIDATOR_PRIVATE_KEY` via environment
   variable is fine for testnets; production validators should use an HSM,
   OS keyring, or remote signer (see `docs/key_management.md`).

4. **State growth & snapshots.** State is held in memory and replayed from
   full block history on startup. Mainnet needs state snapshots / checkpoint
   sync and pruning so startup and memory do not grow unbounded.

5. **External security audit + adversarial testnet.** Independent audit of
    consensus and cryptography, plus a public incentivized testnet with
    adversarial validators, before any mainnet genesis.

6. **Observability & operations.** Structured logging, alerting on reorgs
    and validation failures, snapshot/restore tooling, and documented
    incident-response runbooks.

## Suggested order of work

`P2P identity` → `snapshots/pruning` → `fee market + emission policy` →
`audit + adversarial testnet`.
