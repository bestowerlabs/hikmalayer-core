# hikmalayer

The official Python SDK for the [Hikmalayer](https://hikmalayer.com) blockchain — keys, native signing, transactions, tokens and the AMM DEX.

Mirrors the [JavaScript SDK](../sdk) exactly: same canonical signing domains, same AMM arithmetic, same network scoping. A transaction signed here is byte-identical to one signed by `hikma-wallet` or the JS SDK.

---

## Install

```bash
pip install hikmalayer
```

From source:

```bash
cd python-sdk
pip install -e ".[dev]"
```

Requires Python 3.9 or later.

---

## Quick start

```python
from hikmalayer import HikmalayerClient, LocalSigner, parse_hkm, format_hkm

signer = LocalSigner.random()
client = HikmalayerClient("http://127.0.0.1:3000", signer=signer)

print(signer.address)                        # hkm…

stats = client.stats()
print(format_hkm(stats["total_supply"]), "HKM in circulation")

client.transfer(to="hkm…", amount=parse_hkm("10"))
```

---

## Amounts

Every on-chain amount is an integer in **base units**. `1 HKM = 1_000_000` base units (6 decimals); each native token declares its own.

Python's `int` is arbitrary precision, so there is no 2⁵³ cliff — but the same discipline still applies. Never use a float for an amount:

```python
from hikmalayer import parse_hkm, format_hkm, parse_units, format_units

parse_hkm("1.5")          # 1_500_000
format_hkm(1_500_000)     # '1.5'

parse_units("1.5", 8)     # 150_000_000  — a token with 8 decimals
format_units(150_000_000, 8)  # '1.5'
```

Passing a float is refused rather than rounded:

```python
client.transfer(to="hkm…", amount=1.5)
# TypeError: A float amount has already lost precision.
```

Excess precision is refused too — you cannot accidentally sign an amount the asset cannot represent:

```python
parse_units("1.1234567", 6)
# ValueError: At most 6 decimal places for this asset
```

---

## Keys and signing

The private key never leaves the process. Signing reproduces the chain's scheme exactly, so the node verifies a Python signature identically to a `hikma-wallet` one.

```python
from hikmalayer import LocalSigner, derive_address, derive_public_key

signer = LocalSigner.random()                  # new key from the platform CSPRNG
signer = LocalSigner.from_private_key("…")     # import an existing key

signer.address       # hkm + 40 hex
signer.public_key    # 04 + 128 hex (uncompressed)
```

Every signature is bound to a network, so a transaction signed against the devnet can never be replayed on mainnet. The client fetches the chain id once and scopes each message automatically.

---

## Reading the chain

```python
client.stats()             # height, difficulty, finality, supply, base fee
client.state()             # state root, supply, validator and account counts
client.blocks(0, 10)       # paginated
client.block(5)            # one block in full
client.pending()           # the mempool
client.fees()              # the current dynamic base fee
```

Explorer views:

```python
client.overview()                    # for an explorer home page
client.search("hkm…")                # full history for an address
client.search("c1237035-f8ae-…")     # one transaction
client.block_by_hash("00c8c7e2…")
```

---

## Transfers

```python
client.transfer(to="hkm…", amount=parse_hkm("10"))

client.balance()               # the signer's balance
client.balance("hkm…")         # anyone's

client.nonce()                 # next nonce and current base fee
```

Queued transactions execute when mined:

```python
before = client.stats()["total_blocks"]
client.transfer(to="hkm…", amount=parse_hkm("10"))
client.wait_for(lambda s: s["total_blocks"] > before)
```

A malformed recipient is refused before anything is signed:

```python
client.transfer(to="hkm-typo", amount=parse_hkm("10"))
# HikmalayerError: 'hkm-typo' is not a valid hkm address
```

---

## Native tokens (HTS)

Supply is fixed at creation and can only ever be reduced by burning — no issuer can silently inflate a token.

```python
result = client.create_asset(
    symbol="GOLD",
    name="Hikma Gold",
    decimals=6,
    initial_supply=parse_units("1000000", 6),
)

client.assets()                            # the whole registry
client.asset(token_id)                     # metadata
client.asset_balance(token_id)             # the signer's holding

client.transfer_asset(token_id=token_id, to="hkm…", amount=parse_units("500", 6))
client.burn_asset(token_id=token_id, amount=parse_units("100", 6))
```

Constraints: symbol ≤ 12 characters, name ≤ 64, decimals 0–18. Every token operation pays its fee in HKM.

---

## The AMM DEX

Constant-product pools, each pairing a native token with HKM. Reserves live in a protocol account, so HKM and token supply stay conserved across every operation.

### Swapping

`min_out` defaults to the on-chain quote reduced by a 0.5% tolerance, so a swap always carries a slippage bound even if you forget to set one:

```python
quote = client.quote(token_id, hkm_to_token=True, amount_in=parse_hkm("100"))
print(format_units(quote["amount_out"], 6), "GOLD expected")

client.swap(token_id=token_id, hkm_to_token=True, amount_in=parse_hkm("100"))

# Tighter tolerance — 10 bps
client.swap(token_id=token_id, hkm_to_token=True,
            amount_in=parse_hkm("100"), slippage_bps=10)

# Or set the floor yourself
client.swap(token_id=token_id, hkm_to_token=True,
            amount_in=parse_hkm("100"), min_out=47_000_000)
```

### Providing liquidity

The first depositor sets the price. Later deposits must match the pool ratio and are bound by the scarcer side.

```python
client.add_liquidity(
    token_id=token_id,
    amount_hkm=parse_hkm("1000"),
    amount_token=parse_units("5000", 6),
)

position = client.lp_position(token_id)
client.remove_liquidity(token_id=token_id, shares=position["shares"] // 2)

client.pools()          # every active pool
client.pool(token_id)   # reserves and total shares
```

### Predicting the outcome offline

The SDK mirrors the chain's AMM arithmetic exactly, so you can preview a deposit or withdrawal without a round trip — and verify afterwards that consensus agreed:

```python
pool = client.pool(token_id)

predicted = HikmalayerClient.quote_add_liquidity(
    pool, parse_hkm("1000"), parse_units("5000", 6))
print(predicted["minted"], "shares expected")

out = HikmalayerClient.quote_remove_liquidity(pool, shares=1_000_000)
print(out["amount_hkm"], out["amount_token"])
```

---

## Hybrid (quantum-ready) accounts

A hybrid account is authorised by two signatures over the same message —
secp256k1 ECDSA and ML-DSA-65 (FIPS 204). An attacker has to break both to
forge one transaction.
Both derive from the one private key you already hold, so there is no second
secret to back up.

```python
from hikmalayer import derive_hybrid_identity, pq_available

if pq_available():
    identity = derive_hybrid_identity(private_key)
    identity["address"]         # hkq…
    identity["public_key"]      # 04…  (65 bytes)
    identity["pq_public_key"]   # …    (1952 bytes)
```

Needs `cryptography` 46 or later:

```bash
pip install 'hikmalayer[pq]'
```

Verification works in full:

```python
from hikmalayer import verify_hybrid

verify_hybrid(
    message, address,
    public_key, signature,
    pq_public_key, pq_signature,
)
```

`verify_hybrid` checks three things: that the address is the one both public
keys derive to, that the ECDSA signature verifies, and that the ML-DSA
signature verifies. The address check is what makes the pair binding —
without it, an attacker who broke secp256k1 could pair the victim's classical
key with an ML-DSA key of their own and both signatures would still verify.

### Signing hybrid accounts is not supported here

```python
from hikmalayer import HybridSigningUnavailable

try:
    client.transfer(to="hkm…", amount=parse_hkm("1"))   # from an hkq account
except HybridSigningUnavailable as err:
    print(err)   # explains why, and where to sign instead
```

The chain signs ML-DSA with FIPS 204's `rnd` parameter pinned to
`SHA256(domain ‖ seed ‖ message)` rather than fresh randomness. That makes
signatures reproducible across implementations, and means a broken RNG on the
signer's machine cannot leak the key.

Reproducing it requires supplying `rnd`, and no Python ML-DSA implementation
exposes it:

| Library | Why not |
|---|---|
| `pyca/cryptography` | `sign(data, context)` and `sign_mu(mu)` — no `rnd` |
| `dilithium-py` | Author states it is educational and not constant-time |
| `pqcrypto` | 0.4.0 accepted a tampered ML-DSA-65 message as valid |
| `liboqs-python` | Upstream documents it as prototypical |

Signing with hedged randomness instead would produce valid signatures the
chain accepts — but not byte-identical to the CLI, and Python would be the one
client whose signatures depend on the local RNG. That difference is invisible
in the API and only shows up when someone's entropy is poor, so this SDK
raises instead.

**To sign for a hybrid account, use the JavaScript SDK or `hikma-wallet`.**
Python derives the addresses and verifies the signatures.

---

## Vesting

Cliff plus linear release, enforced by consensus. Once mined the schedule cannot be altered.

```python
client.vest(
    to="hkm…",
    amount=parse_hkm("100000"),
    cliff_blocks=1_314_000,      # ~6 months at 15s blocks
    duration_blocks=4_204_800,   # ~2 years
)

client.vesting("hkm…")
```

---

## Staking

The minimum stake is 10,000 HKM. `vrf_public_key` comes from `hikma-wallet sign-stake` and binds the VRF key used for leader election.

```python
client.stake(amount=parse_hkm("10000"), vrf_public_key="04…")
client.validators()
client.withdraw(amount=parse_hkm("10000"))
client.unbonding()
```

Withdrawn stake stays slashable through the unbonding period.

---

## Admin and P2P

```python
admin = HikmalayerClient(url, admin_token="…", p2p_token="…")

admin.faucet(to="hkm…", amount=parse_hkm("10000"))   # devnet/testnet
admin.set_governance(finality_depth=8)

admin.peers()
admin.register_peer("http://validator2:3000")
admin.checkpoint_bundle()      # a fresh node can boot from this
```

---

## Errors

Node-side rejections are raised, never swallowed. The node returns HTTP 200 with an error body for business-logic failures, so checking the status code alone is not enough — the client handles that for you.

```python
from hikmalayer import HikmalayerError

try:
    client.transfer(to="hkm…", amount=parse_hkm("1000000000"))
except HikmalayerError as err:
    print(err)             # 'Insufficient balance for hkm…'
    print(err.endpoint)    # '/tokens/transfer'
    print(err.body)        # the full response
```

---

## Tests

**Unit suite** — offline, runs in under a second:

```bash
pytest tests/ --ignore=tests/integration -q
```

92 tests covering amount handling, key derivation, every canonical signing
domain, and the AMM arithmetic. The signing domain tests assert the exact
strings — if one changes, every previously signed transaction of that type
stops verifying, so they are checked literally rather than trusted.

**Integration suite** — drives a live devnet end to end:

```bash
# In one terminal, and leave it running:
ops/devnet.sh

# In another, once it reports the 30B genesis balance:
HIKMALAYER_ADMIN_TOKEN=devadmin pytest tests/integration/ -q
```

32 tests covering transfers, token issue/transfer/burn, the full DEX
lifecycle, vesting and error handling. It skips itself when no node is
reachable, so the offline suite stays green either way.

Expect it to take five or six minutes. Every test funds its own account,
issues its own token and seeds its own pool, then waits for the specific
effect it is asserting on to become observable on chain. Sharing state or
assuming one block is enough makes the suite fail intermittently for
reasons that have nothing to do with the SDK — so it does neither.

---

## Parity with the JavaScript SDK and CLI

Verified for the same fixed key:

| | Value |
|---|---|
| Address | `hkm13320761030a4c59d96060708e2377bc4e936dee` |
| Transfer domain | `hikmalayer-transfer:hkm1332…:hkm0000…0001:1500000:7` |

Both match the JS SDK and `hikma-wallet` exactly.

---

## License

MIT

© 2026 Bestower Labs Limited
