# Consensus Flow — Hybrid PoS → PoW

**Status:** Current. Describes the flow as implemented.

Neither stake alone nor hashpower alone produces a block. Stake decides *who*
may produce; work decides *that* it was produced.

---

## 1. The flow

```
   parent block
        │
        ├─► randomness beacon ──► slot input for height N, round R
        │                              │
        │                              ▼
        │                    stake-weighted VRF selection
        │                    over the ON-CHAIN validator set
        │                    at the PARENT state
        │                              │
        │                              ▼
        │                     the selected leader:
        │                       1. builds the block
        │                       2. proves the VRF for this slot
        │                       3. mines it to the current difficulty  ◄── PoW
        │                       4. signs the block hash (ECDSA)
        │                       5. …and with ML-DSA-65 if it is a hkq account
        │                              │
        ▼                              ▼
   every node re-checks:  selection · VRF proof · PoW · signature(s)
                          · timestamp bounds · state root by RE-EXECUTION
```

## 2. Step by step

**1 — Validators register.** `Stake` bonds HKM and registers the validator's
secp256k1 key, its sr25519 VRF key, and — for a `hkq…` validator — its
ML-DSA-65 key, all in the on-chain staker set. A minimum stake applies. When a
genesis allowlist is configured, only listed addresses may join.

**2 — Leader selection.** The slot input comes from the randomness beacon at the
parent state, salted with the height so one parent hash can never be reused to
claim a different slot. Selection is stake-weighted over the validator set **as
of the parent state**, not as of the producer's local view.

**3 — Liveness rotation.** Round 0's leader is the primary. Each elapsed slot
timeout (30s) opens the next round's leader as a fallback, so an offline
validator delays the chain by at most one timeout rather than stalling it. A
block must come from the **smallest open round** that selects its producer, and
its VRF must verify against exactly that round's slot input.

**4 — Proof of Work.** The selected leader mines the block itself, to a
difficulty derived deterministically by the retargeting schedule and clamped to
1–5. There is no external miner and none is needed; hashrate without stake
produces nothing.

**5 — Signing.** The leader signs the block hash with its registered key. A
`hkq…` validator signs with **both** schemes; the block carries
`validator_pq_signature`, and the key registered on chain — not anything in the
block — decides whether one is required.

**6 — Validation.** Every node independently checks:

- the producer is selected for the smallest open round at the parent state;
- the VRF proof verifies against that round's slot input and the registered
  VRF key;
- the Proof-of-Work meets the consensus-derived difficulty;
- the block signer's key **equals** the key registered on chain;
- the classical signature verifies, and the post-quantum one too whenever the
  validator's registered ML-DSA key is set (and is **absent** when it is not);
- timestamps are bounded in both directions — never before the parent, and
  within the clock-skew bound — which closes retarget manipulation;
- the Merkle root matches the payload, and the **state root matches
  re-execution**.

Failures are classified: provable misbehaviour (wrong slot, bad signature, bad
PoW, tampered payload or state) is **slashable**; structural problems (missing
fields, stale tip) are not.

## 3. Fork choice

**Validator-progress first.** Finalized blocks are irreversible. A fork must
carry *more validator-sealed blocks* to displace the local chain; cumulative
Proof-of-Work only breaks exact ties. Fork tips future-dated beyond the
clock-skew bound are rejected outright.

An adopted chain is **re-executed under local network parameters** and its state
rebuilt from genesis. A candidate's claims about its own genesis are never
trusted — that is what stops a peer presenting a chain from a different network
with a plausible-looking history.

## 4. Slashing and unbonding

Equivocation proofs are permissionless: anyone can submit one, and it burns the
offender's stake on chain. Double-slashing is prevented. Withdrawn stake stays
locked and slashable for the unbonding period, and the slashing window equals
it — so misbehaving stake can never exit ahead of its punishment.

## 5. Transport

Inter-node propagation uses a versioned protocol envelope
(`POST /p2p/protocol`, `hikmalayer-p2p/1`) with typed payloads (`Ping`,
`PeerAnnounce`, `Block`, `BlockBatch`). Every envelope is signed by the sender's
node key and bound to its derived `node_id`; `P2P_REQUIRE_IDENTITY=true` rejects
unsigned envelopes. A bounded message-id cache provides replay protection, and
peer reputation scoring auto-bans repeat offenders.
