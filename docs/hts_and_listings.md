# HKM, HTS, and Exchange Listings

Written because "will HTS tokens be listed on exchanges like HKM?" is a
reasonable question with a nuanced answer, and the nuance matters more than the
headline.

---

## 1. HKM and HTS are not the same kind of thing

| | **HKM** | **HTS token** |
|---|---|---|
| What it is | The chain's native coin | An asset issued *on* the chain |
| Who creates it | Nobody — it exists at genesis and through block rewards | Anyone, permissionlessly, with one transaction |
| Supply | Consensus emission schedule: genesis allocation + mined rewards + perpetual tail | Fixed at creation by the issuer; reducible only by burning |
| Pays fees | Yes — every transaction fee is in HKM | No |
| Secures the chain | Yes — staking and block rewards are HKM | No |
| Analogy | ETH, BNB, SOL | ERC-20 — but a consensus object, not a contract |

Every HTS pool on the native AMM pairs against HKM. HKM is the settlement asset
of its own economy.

---

## 2. What "listed" means, and the two venues

### 2.1 The native AMM — live today, no permission needed

Any HTS token can be traded the moment its issuer seeds a pool. This is not a
listing process: there is no application, no review, no fee, and no gatekeeper.
Create the token, add liquidity, and it trades.

```bash
# issue, then seed a pool — two signed transactions
hikma-wallet sign-token-create <SYMBOL> <name> <decimals> <supply> <nonce> <key>
hikma-wallet sign-amm-add <token_id> <hkm> <tokens> <min_shares> <nonce> <key>
```

**This is the venue that exists.** For most HTS tokens it is the only one they
will ever need or get.

### 2.2 Centralized exchanges — possible, but understand the barrier

Nothing in the protocol prevents a centralized exchange from listing HKM or any
HTS token. Technically it is straightforward: HTS assets are first-class
consensus objects with stable ids, queryable balances, and the same
authorization model as HKM, so an exchange integrates them the same way it
integrates the native coin — one integration covers both.

The barrier is not technical, and it is not small:

**There is no bridge, and none is planned.** An exchange cannot reach
Hikmalayer through an existing EVM or Cosmos integration. It must run a
Hikmalayer node and integrate the REST API directly — bespoke work, for one
chain, that no existing tooling covers. That is a real cost an exchange weighs
against expected volume, and it is the reason most chains this size are not
listed anywhere.

This is a deliberate trade, not an oversight. Bridges are the most-attacked
component in the industry (Ronin $600M, Wormhole $320M, Nomad $190M).
Declining one removes that entire attack surface, along with a dedicated audit
budget, 24/7 signer operations, and custody-related legal exposure. The cost is
exactly the friction described above. See `bridge_design.md`.

**Listing is a business decision, not a protocol feature.** No blockchain can
cause an exchange to list an asset. Exchanges apply their own criteria —
liquidity, volume, legal review, custody engineering — and the protocol has no
say in any of it.

---

## 3. Honest expectations for an HTS issuer

- **You can create a token and trade it immediately** on the native AMM. That
  is real, permissionless, and needs nobody's approval.
- **You should not assume a centralized listing.** Any chain where token
  creation is permissionless has far more tokens than any exchange will ever
  list, and Hikmalayer's lack of a bridge raises the integration cost further.
- **Permissionless creation means "anyone can make one", not "anyone should
  trust one".** A token's symbol and name are attacker-controlled strings. The
  wallet strips invisible and bidirectional characters and flags symbols
  imitating HKM, but the only durable identifier is the `hkt…` token id. Check
  the id.
- **What consensus does guarantee** about every HTS token, without an audit:
  fixed supply with no mint function, no blacklist, no transfer hook, no
  upgradeable proxy, no admin key. There are no contracts, so there is nothing
  for those things to hide in. The same absence is why an HTS token cannot have
  legitimate custom behaviour either — no rebasing, no fee-on-transfer.

---

## 4. What would have to change

For HTS tokens to be broadly tradeable outside Hikmalayer, one of these would
have to happen:

1. **Exchanges integrate Hikmalayer natively.** Possible today; it is a
   business-development question, and volume drives it.
2. **A bridge is built.** Explicitly declined. Reasoning, and the conditions
   under which it would be safe to revisit, are in `bridge_design.md`. Nothing
   in this repository is work toward one.

No public material should describe external assets (BTC, ETH, USDT, …) as
tradeable, bridgeable, or wrapped on Hikmalayer. They are not, and will not be.
