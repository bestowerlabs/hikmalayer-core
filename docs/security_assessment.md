# Hikmalayer — Security Assessment

**Scope:** consensus and state machine, node API, wallet (browser + extension),
AMM DEX, and the developer SDK.
**Method:** code review plus an adversarial test suite (`tests/security.rs`)
that plays an attacker against the real interfaces, and live verification
against a running chain.
**Status:** every finding below is **fixed**, with a regression test that fails
on the pre-fix code.

This is an internal assessment. It is not a substitute for an independent
audit, and it does not claim completeness — it records what was looked for,
what was found, and what changed.

---

## Summary

| # | Finding | Severity | Status |
|---|---|---|---|
| 1 | Integer overflow in fee arithmetic mints supply from nothing | **Critical** | Fixed |
| 2 | No network binding — signatures replay across networks | **High** | Fixed |
| 3 | Unvalidated recipient addresses destroy funds silently | **High** | Fixed |
| 4 | `apply_transaction` trusted callers to have verified signatures | **Medium** | Fixed |
| 5 | Block height frozen on an idle chain (emission and vesting stall) | **Medium** | Fixed |
| 6 | Client-side amount rounding breaks honest transactions | **Medium** | Fixed |
| 7 | Liquidity provision shipped without slippage bounds | **Medium** | Fixed |
| 8 | Network check missed the block-validation path | **High** | Fixed |
| 9 | Mempool projection re-verified every pooled signature | **Medium** | Fixed |

Verified as **sound** and left unchanged: sender/key binding, nonce replay
protection, cross-domain signature separation, the constant-product invariant,
LP share accounting, minimum-liquidity locking, vesting cliffs, stake
minimums, and state-root determinism.

---

## 1. Integer overflow in fee arithmetic mints supply — **Critical**

**Where:** `ChainState::apply_transaction`, the `Transfer`, `Stake`, `Vest`,
`AddLiquidity` and `Swap` arms.

Balance checks were performed on `amount + fee` using unchecked `u64`
addition. Rust disables overflow checks in release builds by default — which
is what a validator runs — so the addition wrapped.

An attacker picks `amount = 2^64 − fee`. Then `amount + fee` wraps to `0`, the
balance check passes trivially, and the **full** amount is credited to the
recipient.

Measured on the pre-fix code, in a release build:

```
attacker balance before : 10000        (0.01 HKM)
attacker balance after  : 10000        (paid nothing, not even the fee)
recipient started with  : 0
recipient now holds     : 18446744073709550616
                          = 18,446,744,073,709 HKM
entire chain max supply :        100,000,000,000 HKM
apply_transaction returned: Ok(())
```

An account holding one hundredth of a coin created **184× the entire supply**,
and the transaction was accepted. Every node computes the same wrong answer,
so consensus never notices; the ledger is simply wrong from that block on.

In a debug build the same expression panics instead — a remote denial of
service any account can trigger. Neither outcome is acceptable.

**Fix.** Every balance-affecting operation in the state machine now uses
checked arithmetic and returns an error rather than wrapping (`try_credit`,
`try_credit_token`, `total_debit`, and explicit `checked_add` on stake, pool
reserves and LP shares). Amounts the state machine has already bounded
saturate rather than wrap. `overflow-checks = true` is now set for release
builds as a backstop: a halted node is recoverable, a corrupted ledger is not.

**Regression tests:** `an_overflowing_transfer_cannot_mint_supply`,
`an_overflowing_stake_cannot_mint_voting_power`,
`crediting_beyond_u64_is_rejected_rather_than_wrapping`,
`add_liquidity_that_overflows_a_reserve_is_rejected`. All three fail on the
pre-fix code and pass now, in both profiles.

---

## 2. No network binding — signatures replay across networks — **High**

**Where:** every canonical signing message.

Addresses are derived from the key, so a user has the **same address** on a
testnet and on mainnet. Nothing in a signed message named the network:

```
hikmalayer-transfer:hkm1332…:hkm0000…:1000000:1
```

That string, and its signature, were valid on any Hikmalayer network. The
realistic attack needs no sophistication: a user tries the chain on a testnet
first (a transfer at nonce 1), later funds the same key on mainnet, and
anyone who saw the testnet transaction replays it verbatim. Their mainnet
nonce is 1 too, so it applies.

This is the problem EIP-155 solved for Ethereum in 2016.

**Fix.** Transactions carry a `chain_id`, fixed at genesis
(`GENESIS_CHAIN_ID`) and committed to by the state root. It is part of the
signed message as a **visible prefix**:

```
hikmalayer-mainnet:hikmalayer-transfer:hkm1332…:hkm0000…:1000000:1
```

A prefix rather than a hidden change to the digest, so a wallet's confirmation
screen shows the user which network they are authorizing. The node stamps its
own id on submission, so a signature made for another network fails
verification; relabelling the transaction invalidates the signature.

The default is `hikmalayer-dev` — deliberately, so an unconfigured node never
looks like mainnet.

**Regression tests:** `a_transaction_signed_for_another_network_is_refused`,
`relabelling_the_network_invalidates_the_signature`,
`a_transaction_on_its_own_network_still_applies`, plus SDK conformance checks
that the same transaction signs differently per network.

---

## 3. Unvalidated recipient addresses destroy funds — **High**

**Where:** `Transfer`, `Vest` and `TokenTransfer`.

Balances are keyed by string, and no recipient was validated. Sending to a
mistyped address created a balance under that string — an account nobody holds
a key to. Demonstrated on a live node:

```
$ curl -X POST /tokens/transfer -d '{… "to":"typo-address" …}'
{"status":"success", …}

$ curl /tokens/balance/typo-address
{"account":"typo-address","balance":1000000}
```

There is no checksum to catch it and no way to reverse it. This was found
while building the SDK, which is exactly the point of building one.

**Fix.** `is_valid_address` (43 chars, `hkm` + 40 lowercase hex) is enforced
in consensus on every user-supplied recipient. Internal pool accounts
(`__staking_pool__`, `__amm_pool__`, `__vesting_pool__`) are not addressable
by users, so pool accounting cannot be perturbed from outside. The SDK checks
before anything is signed, so the error names the offending field.

**Regression tests:** `funds_cannot_be_sent_to_a_malformed_address`,
`internal_pool_accounts_are_not_user_addressable`,
`recognizes_well_formed_addresses`, `transfer_to_a_malformed_address_is_rejected`.

---

## 4. `apply_transaction` trusted its callers — **Medium**

`ChainState::apply_transaction` is public and did not verify signatures; it
relied on every caller having called `verify_for_block` first. All callers did,
so this was not exploitable — but it made writing a forged transaction into
the ledger one forgotten call away, and the inconsistency was live already
(the `Withdraw` arm verified inside `apply`, the others did not).

**Fix.** `apply_transaction` now verifies authorization itself via
`Transaction::verify_authorization`. Block validation and block production —
which have just run the full check — use `apply_verified`, so the hot path
does not verify twice.

**Regression test:** `a_transfer_signature_cannot_authorize_a_stake` (which
caught this: signature verification rejected the cross-domain replay, but
`apply_transaction` accepted it).

---

## 5. Block height frozen on an idle chain — **Medium**

`plan_block` refused to produce a block with no pending transactions. Block
height is the chain's clock: the emission schedule is defined per height (a
halving every 9,500,000 blocks **assumes ~15s blocks**), vesting releases per
block, unbonding completes per block, and the slashing window is measured in
blocks.

So on a quiet chain nothing advanced. Locked funds never released, unbonding
never completed, and emission fell arbitrarily behind the published schedule —
the 30/70 premine-to-mined split over ~18 years would simply not happen.
Observed directly: the demo application's vesting step hung, because the chain
had stopped producing blocks.

**Fix.** Reward-only blocks are produced. The chain now advances on its own —
verified live: height moved from 4 to 6 across 12 seconds with zero traffic.
This also keeps the validator's incentive aligned, since there is always a
reason to build the next block.

---

## 6. Client-side amount rounding breaks honest transactions — **Medium**

Amounts were passed through JavaScript's `Number` before being submitted. The
supply is 10^17 base units, well past `Number.MAX_SAFE_INTEGER` (≈9.007×10^15),
so large amounts lost their low digits. Because signatures cover the exact
decimal text of the amount, the rounded value no longer matched what was
signed — the node rejected an honest transfer, and nothing in the error
explained why.

**Fix.** Amount fields accept decimal strings as well as numbers
(`src/api/amount.rs`); a number that is fractional, negative, or beyond
2^53−1 is rejected rather than truncated. Every client sends
`BigInt.toString()`.

Verified live: an amount of 2^53+1 base units is refused when sent the old
way and lands on-chain exactly when sent as a string.

---

## 7. Liquidity provision without slippage bounds — **Medium**

The DEX front-end submitted `min_shares`/`min_hkm`/`min_token` hardcoded to
`1` — no bound at all, so a deposit or withdrawal could be sandwiched.

**Fix.** Clients mirror the chain's AMM arithmetic exactly
(`quoteAddLiquidity` / `quoteRemoveLiquidity`), preview the outcome, and derive
a user-controlled bound from it. The SDK applies a 0.5% bound by default and
has no way to submit without one by accident. Verified against a live pool:
predictions match consensus to the base unit, and an unsatisfiable bound is
rejected by the chain.

---

## 8. Network check missed the block-validation path — **High**

Found while reviewing the fix for finding 2, which is the point of reviewing
a fix rather than trusting it.

`verify_chain_scope` was called from `apply_transaction`. But block validation
applies transactions through `apply_verified` — the path that skips the
redundant signature check — and that path did not run it.

So a validator could put a transaction signed for another network into a
block, and every node would accept it. The signature is genuine; it is simply
for a different chain. That is the exact replay the fix exists to prevent,
arriving by the only route that actually matters, and the ordinary entry point
being protected would have made it look fixed.

**Fix.** The network check moved into `apply_verified`, so every path that
touches state runs it. It is a string comparison, not a signature check —
unlike authorization there was never anything to gain by skipping it.

**Regression test:** `a_block_cannot_carry_a_transaction_from_another_network`,
which asserts the transaction is internally valid first, so it fails for the
right reason.

---

## 9. Mempool projection re-verified every pooled signature — **Medium**

A regression introduced by the fix for finding 4. Making
`apply_transaction` verify signatures was right, but `project_pending_state`
applies every pooled transaction on **every** submission — so each request
cost one secp256k1 verification per transaction already in the mempool.

With the 1,000-transaction cap that is up to 1,000 verifications per request,
triggerable by anyone, repeatedly: O(n) work per call and O(n²) overall, for
no added safety, since mempool contents are verified at admission.

**Fix.** Callers that have just verified use `apply_verified`: the mempool
projection, the block builder, the submission path and the gossip path. Each
signature is now checked exactly once. The network check stays in the shared
path (see finding 8), so none of this weakens replay protection.

---

## What was checked and found sound

**Authorization.** The public key in a transaction must derive to the sender's
address (`verify_sender_signature`), so a signature cannot authorize an
account it does not control. Substituting either field invalidates the
signature. Verified by `cannot_spend_from_an_account_you_do_not_control` and
`cannot_substitute_a_public_key_for_another_account`.

**Replay.** Nonces are strictly sequential per account — no reuse, no skipping
ahead to reserve a slot. Each transaction type has its own message prefix, so
a transfer signature cannot authorize a stake.

**AMM.** The constant product never shrinks across a swap (the 0.30% fee stays
in the pool, accruing to LPs — measured at +0.0074% over the demo's trades).
LP shares cannot be burned by an account that does not hold them.
`MINIMUM_LIQUIDITY` keeps `total_shares` above zero permanently, so the
first-depositor share-inflation attack stays closed.

**Economics.** Supply is conserved across ordinary activity. Stake below the
validator minimum cannot join the set. Withdrawing more stake than is bonded
fails. Vested funds are not spendable before the cliff.

**Determinism.** The state root commits to every base unit — a one-unit
transfer changes it — and identical histories produce identical roots.

---

## Residual risks

These are properties of the design, not defects, and are stated so nobody is
surprised by them.

- **No independent audit.** Everything here was found by the same people who
  wrote the code. An external review is the obvious next step, and the
  adversarial suite is a starting point for one, not a replacement.
- **Permissioned launch posture.** `GENESIS_VALIDATOR_ALLOWLIST` gates who may
  join the validator set. Opening it is a governance decision with real
  security consequences — a small permissionless validator set is cheap to
  outvote.
- **Wallet keys.** The extension holds keys in the extension's own context,
  which stops a website (or an XSS in one) reading them. It does not stop
  malware on the device. Treasury and validator keys belong offline.
- **No seed phrase.** Each account is an independent key; each needs its own
  backup. HD derivation from one recovery phrase is not implemented.
- **Extension not published or externally reviewed.** It is the piece users
  trust with their keys and should be reviewed before it ships.
- **Admin endpoints are token-gated, not signature-gated.** Anyone holding
  `ADMIN_TOKEN` can drive them. Treat it as a production secret.

---

## Reproducing

Two of the nine findings (8 and 9) were introduced or exposed by fixing the
others. That is normal, and it is why the fixes were reviewed as carefully as
the original code: a fix that looks right from the entry point everyone reads
can still miss the path that matters.

```bash
cargo test --test security             # adversarial suite, debug
cargo test --release --test security   # and in the profile validators run
cargo test                             # full suite (117 unit tests)
cargo clippy --all-targets             # clean

ops/devnet.sh &                        # a live chain
node examples/token-launch/launch.mjs  # end-to-end application
cd sdk && HIKMALAYER_ADMIN_TOKEN=devadmin npm run test:integration
```
