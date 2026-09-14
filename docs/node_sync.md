# Node Synchronisation

How a Hikmalayer node joins a running network and stays on it — automatically,
with no manual bootstrap.

---

## 1. What it does

A node needs nothing but the address of one peer:

```bash
BOOTNODES=http://bootnode:3000 ./hikmalayer
```

From that it will, on its own:

1. **Announce itself** to its peers, so their gossip reaches it.
2. **Learn peers from peers**, so the network grows past the bootnodes it
   started with.
3. **Poll every peer's head** — a small summary, not a chain download — to see
   whether anyone is ahead, and whether they are even on the same network.
4. **Pull the history it is missing** in bounded batches, validating every
   block itself, until it reaches the tip.
5. **Keep doing all of the above**, so a node that falls behind recovers the
   same way it caught up in the first place.

A brand-new validator reconstructs the entire chain from genesis unattended.
No state file copying, no manual checkpoint import, no operator intervention.

---

## 2. The problem this replaced

Before this existed there was exactly one code path that could bring a node up
to date, and it ran in exactly one situation: a **gossiped** block arrived and
failed to extend the local tip. That serves a node that is already current and
missed a block. For a node that has never seen the chain, two gaps compounded:

- **Nothing ever asked a peer for history.** A fresh node sat at genesis
  waiting for a block it had no reason to receive.
- **Nothing ever announced the node to its peers.** `PeerAnnounce` existed in
  the protocol and was handled on receipt, but **no node ever sent one**. So
  producers did not know the new node existed, never gossiped to it, and the
  one trigger that did exist could never fire.

The observable symptom: a fresh validator sitting at genesis indefinitely,
making no attempt to sync, requiring its state file to be copied across by
hand. That bootstrap was one-time — the node still would not advance on its
own afterwards.

Both gaps are closed. The fix is not "gossip harder"; it is that a node now
**pulls**, which does not depend on anyone knowing it exists.

---

## 3. How it works

```
  every P2P_SYNC_INTERVAL_SECONDS (default 10s)
        │
        ├─ announce self to peers ──────────► they can now gossip to us
        ├─ ask peers for their peers ───────► network grows past bootnodes
        │
        ├─ GET /p2p/head from each peer      (small: no chain download)
        │     └─ drop any peer whose chain_id / root / base height differ
        │
        ├─ is the best peer ahead of us?
        │     no  → done
        │     yes ↓
        │
        └─ loop:  GET /p2p/blocks/{from}?limit=256
                    └─ for each block: validate_block_candidate → commit
                         └─ on divergence: stop, hand to fork choice
```

### Endpoints

| Endpoint | Purpose |
|---|---|
| `GET /p2p/head` | Chain id, root hash, base height, tip height and hash, finalized height, cumulative work. Everything needed to decide whether to sync, and from whom |
| `GET /p2p/blocks/{from}?limit=N` | Blocks from an absolute height, capped at 256 by the **server**. `limit` may only ask for fewer |
| `GET /p2p/chain` | The whole chain. Still used, but only as the fork-choice fallback |

All three are P2P-gated (`x-p2p-token`), consistent with the rest of `/p2p/*`.

The head endpoint is why polling is cheap. Downloading a chain to compare two
heights would make a background syncer cost more than the chain it syncs.

---

## 4. What it does not do

These boundaries are the point of the design, not omissions.

**It does not weaken validation.** Every block fetched by the syncer goes
through `validate_block_candidate` — the same full re-execution a gossiped
block gets: PoS selection, VRF proof, Proof-of-Work, both signatures where
applicable, timestamp bounds, and the state root re-derived by executing the
block. Sync is a *transport*. It buys no trust. A peer that serves a bad block
gets it rejected exactly as if it had gossiped it.

**It does not bypass fork choice.** Catching up only ever *appends to our own
tip*. The moment a peer's history diverges, the fast path stops and
`try_adopt_chain` decides — which is where finalized-history protection and
validator-progress-first fork choice live. **Sync can make us longer; only
fork choice can make us different.**

**It does not cross networks.** Chain id, root hash and base height are
compared before a single block is downloaded. A peer on another network costs
one small request and is then ignored.

**It does not freeze the node.** Blocks are fetched outside the chain lock and
applied under a brief one. A syncing node keeps serving requests instead of
stalling for the length of a download.

**It does not trust a peer's word about how much to send.** The 256-block cap
belongs to the server, so one caller cannot turn a range request into a
memory-exhaustion request.

---

## 5. Configuration

| Variable | Default | Purpose |
|---|---|---|
| `BOOTNODES` | — | Comma-separated peer URLs to start from. One is enough |
| `P2P_PUBLIC_URL` | derived from `NODE_ID` | How peers reach **this** node. Announced to them so gossip flows both ways |
| `P2P_SYNC_INTERVAL_SECONDS` | `10` | Gap between ticks when caught up |
| `P2P_SYNC_DISABLED` | unset | `1` turns the syncer off. Only for an isolated single-node devnet |

### About `P2P_PUBLIC_URL`

If unset, the node falls back to `http://{NODE_ID}:{PORT}` when `NODE_ID`
plausibly resolves as a hostname — which is what a container or service name
gives you. It refuses to guess from a label like the default `node-local`.

**Sync works without it.** Announcing is an optimisation for being gossiped
*to*; pulling does not depend on anyone knowing you exist. A node that cannot
name itself still catches up, just slightly later — it learns of new blocks on
its next poll rather than the moment they are produced. The startup banner says
which mode is active.

---

## 6. Checkpoint fast-sync is a different thing

`GET /checkpoint/bundle` and `HIKMALAYER_CHECKPOINT` still exist and are
unchanged. They serve a different purpose:

| | Automatic sync | Checkpoint fast-sync |
|---|---|---|
| Trigger | Always on | Manual: fetch a bundle, set an env var |
| History | Replays **everything** from genesis | Starts at a trusted anchor |
| Trust | None — verifies it all | Weak subjectivity: you trust the anchor |
| Use | The default for every node | Very long chains where full replay is too slow |

Automatic sync is the default and the trust-minimising path. Checkpoint
fast-sync remains an explicit, opt-in shortcut for operators who accept the
weak-subjectivity assumption.

Note: `/checkpoint/bundle` is **P2P-gated** (`x-p2p-token`), not admin-gated.
Using an admin token returns 401 — a likely cause of "I couldn't get it to
authenticate".

---

## 7. Verifying it

```bash
# Peer's view of its own chain
curl -s -H "x-p2p-token: $P2P_TOKEN" http://peer:3000/p2p/head

# A window of blocks
curl -s -H "x-p2p-token: $P2P_TOKEN" 'http://peer:3000/p2p/blocks/100?limit=10'

# Did the peer learn about us?
curl -s -H "x-p2p-token: $P2P_TOKEN" http://peer:3000/p2p/peers
```

Two nodes are on the same chain when their `/p2p/head` reports the same
`tip_hash` **and** `GET /blockchain/state` reports the same `state_root`. The
state root is the stronger check: it commits to every balance, staker and
nonce, so matching roots mean the node re-executed the history rather than
merely copying blocks.

### Verified behaviour

Exercised against live nodes:

| Scenario | Result |
|---|---|
| Fresh node, genesis only, one bootnode address | Pulled all 25 blocks, identical tip hash and state root, `is_valid: true` |
| Bootnode learns the new node | `/p2p/peers` lists it — the link that never formed before |
| Chain advances by 10 | Follower reached the new tip on its next tick |
| Node offline for 15 blocks, then restarted | Resumed from its persisted height and caught up |
| Peer on a **different** `GENESIS_CHAIN_ID` | Refused — stayed at height 0 throughout |

Regression tests: `a_fresh_chain_rebuilds_full_history_from_block_ranges`,
`a_head_identifies_the_network_and_ranges_stay_bounded`,
`a_tampered_block_from_a_peer_is_refused_during_catch_up` in
`src/blockchain/chain.rs`, plus the unit tests in `src/p2p/sync.rs`.
