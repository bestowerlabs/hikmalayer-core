# Mainnet Readiness

This document tracks what the current codebase already enforces and what still
stands between this chain and a responsible **public mainnet** launch. It is
kept deliberately honest: a mainnet holds real value, so every open item below
is a launch blocker until closed.

## ✅ Implemented and tested

| Area | Status |
|---|---|
| Hybrid consensus | PoS stake-weighted selection (height-salted seed) + PoW finalization, enforced on every block |
| Block integrity | Deterministic genesis, Merkle root committed into the mined hash, full-chain validation |
| Validator authentication | Block signature must match the validator's **registered** staker key; no server-side keys |
| Transaction security | All transfers signed (raw secp256k1 or Ethereum personal_sign), verified at ingress **and** at consensus level |
| Replay protection | Per-account strictly increasing nonces (API); bounded message-ID cache (P2P) |
| Fork choice | Cumulative-work comparison, full candidate-chain validation, finalized-history protection |
| Sync | Peers serve `/p2p/chain`; nodes auto-sync on tip mismatch |
| Authorization | Deny-by-default admin and P2P tokens; admin-gated faucet, certificates, difficulty, governance, slashing |
| Resource bounds | Difficulty clamped 1–5 (prevents PoW-disable and mining stalls); input length limits |
| Rewards | Fixed block reward, consensus-verified (recipient + amount + at most one per block) |
| Slashing | Provably invalid blocks generate slashing evidence; slashable vs structural errors distinguished |
| Tooling | `hikma-wallet` offline keygen/signing; propose/sign/submit flow for external validators |
| Tests | 44 automated tests covering consensus, security, replay, fork choice, and API flows |

## 🚧 Launch blockers (Phase 6)

1. **On-chain validator-set state machine.** Stake registration is currently
   node-local app state (blocks carry staker snapshots for validation).
   Mainnet requires stake deposits/withdrawals to be on-chain transactions so
   every node derives the identical validator set from the chain itself.

2. **Global transaction execution from blocks.** Balances are updated at the
   ingress node and anchored on-chain; peers verify transaction signatures in
   blocks but do not re-execute them. Mainnet requires deterministic state
   transition from blocks (execute-on-accept with state root commitments),
   including balance re-checks and state rebuild on reorg.

3. **VRF-based leader election.** The selection seed (`parent_hash:height`)
   is deterministic and public, so the current producer can attempt to grind
   block contents to influence the next slot. A verifiable random function
   (or RANDAO-style beacon) is required for unbiasable selection.

4. **Signed peer handshakes / validator networking.** P2P is currently
   authenticated by a shared bearer token, suitable for a permissioned
   testnet. Mainnet requires per-node keypairs, signed handshakes, and
   peer scoring/banning.

5. **Mempool and block-size limits.** Pending-pool size, per-block
   transaction count, and per-request body limits need explicit caps and
   fee-based prioritization (fee market not yet designed).

6. **Difficulty adjustment.** Difficulty is an operator-set constant. A
   retargeting algorithm tied to observed block times is needed.

7. **Economic design.** Fixed 5-token block reward, no fees, no halving
   schedule, no treasury policy. Token economics must be specified before
   value is attached.

8. **Key management hardening.** `VALIDATOR_PRIVATE_KEY` via environment
   variable is fine for testnets; production validators should use an HSM,
   OS keyring, or remote signer (see `docs/key_management.md`).

9. **External security audit + adversarial testnet.** Independent audit of
   consensus and cryptography, plus a public incentivized testnet with
   adversarial validators, before any mainnet genesis.

10. **Observability & operations.** Structured logging, alerting on reorgs
    and validation failures, snapshot/restore tooling, and documented
    incident-response runbooks.

## Suggested order of work

`(1) on-chain validator set` → `(2) execute-on-accept state machine` →
`(6) difficulty retargeting` → `(3) VRF` → `(4) P2P identity` →
`(5) mempool limits + fees` → `(7) economics` → `(9) audit + adversarial testnet`.
