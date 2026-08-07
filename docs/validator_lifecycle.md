# Validator Lifecycle

**Status:** Current. Describes behaviour as implemented.

---

## 1. Choose an account type first

This decision is made **before** staking and cannot be changed afterwards: the
address *is* the key material, so switching means unbonding and staking again.

| | Classical (`hkm…`) | Quantum-ready (`hkq…`) |
|---|---|---|
| Block signature | ECDSA | ECDSA **+ ML-DSA-65** |
| Unbonding | ECDSA | Both, against the keys registered on chain |
| Per-block signature overhead | 64 bytes | 3,373 bytes |
| Remote signer / HSM | Widely supported | **Most cannot produce ML-DSA-65 today** |
| Survives a quantum break of secp256k1 | No | Yes |

A validator's key is the longest-lived key on the chain: it sits in the staker
set, in public, for as long as its stake does. There is no "spend once and
rotate" for it, which is the argument for hybrid. The argument against is the
signer constraint in the last row — until HSMs support ML-DSA, a hybrid
validator's key is a software key. Weigh both.

## 2. Onboarding

```bash
# 1. Generate a key offline. Prints BOTH identities from one secret.
hikma-wallet keygen

# 2. Fund the address you intend to validate with (needs the minimum stake plus fees)

# 3. Bond. The VRF public key is part of what is signed, binding the
#    leader-election identity to the stake.
HIKMALAYER_HYBRID=1 \   # omit for a classical validator
  hikma-wallet sign-stake <address> <amount> <nonce> <private_key>

# 4. Submit to a node
curl -X POST "$NODE/staking/deposit" -H 'content-type: application/json' -d @stake.json
```

Bonding registers, in the on-chain staker set: the stake, the secp256k1 public
key, the sr25519 VRF public key, and — for a `hkq…` validator — the ML-DSA-65
public key. That last key is taken from a transaction whose signature has
already been bound to the sender's address, so it is the key the address commits
to, not merely one the sender claimed.

**Constraints:** the resulting stake must meet the minimum (10,000 HKM). When a
genesis allowlist is configured, only listed addresses may *join* — existing
validators may always top up.

## 3. Active validation

The node needs the validator's key to produce blocks:

```bash
VALIDATOR_PRIVATE_KEY=<hex> ./hikmalayer
```

It then: waits to be selected, proves the VRF for its slot, mines the block to
the consensus difficulty, and signs — with both schemes automatically if its
registered ML-DSA key is set. Nothing else is required; there is no separate
miner infrastructure.

**Liveness:** missing a slot costs one timeout (30s), after which the next
round's leader may produce. Being offline delays the chain; it does not stall
it, and it is not slashable.

## 4. Slashing

Slashable: provable misbehaviour — equivocation (two blocks for one slot), a bad
signature, invalid Proof-of-Work, a tampered payload or state root, or producing
outside the open rounds.

Not slashable: being offline, being slow, or structural problems like a stale
tip.

Equivocation proofs are permissionless — anyone may submit one — and burn the
offender's stake on chain. Double-slashing for one offence is prevented.

## 5. Exit

```bash
HIKMALAYER_HYBRID=1 \   # required for a hkq validator
  hikma-wallet sign-withdraw <address> <amount> <nonce> <private_key>
```

- Verified against the keys **registered on chain**, not against keys supplied
  with the transaction. For a hybrid validator that means both signatures; a
  classical signature alone is refused, which is the point — otherwise a broken
  secp256k1 would take the whole stake even though it could not touch the
  balance.
- A withdrawal must either **exit fully** or leave at least the minimum bonded.
  No half-exits below the floor.
- Withdrawn stake stays **locked and slashable** for the unbonding period, and
  the slashing window equals it, so misbehaving stake cannot exit ahead of its
  punishment.
- Exit completes only when nothing remains bonded or unbonding.

## 6. Key rotation

There is none in place: the address *is* the key. Rotating means unbonding the
old validator and staking a new one — a governance-visible action by design,
because a key that could be swapped silently would be a key an attacker could
swap silently.
