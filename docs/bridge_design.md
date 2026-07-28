# Brick 3 — Cross-Chain Bridge: Scope and Security Design

**Status: NOT PURSUED — decision taken. Design retained for reference only.**

The project has decided **not to build a cross-chain bridge**. No bridge code
exists in this repository and none is planned. Hikmalayer's DEX trades HKM
and Hikmalayer-issued (HTS) tokens only.

This document is kept because the reasoning stays useful: it records *why*
the decision is sound, and what would have to be true before anyone revisits
it. Nothing here should be read as work in progress.

**Consequence for all public materials:** external assets (BTC, ETH, USDT,
SOL, …) are never to be shown as tradeable, bridgeable, or wrapped on
Hikmalayer. See §8.

This document exists because "list the top 100 coins on the Hikmalayer DEX"
is a reasonable business goal with an unreasonable naive implementation. It
sets out honestly what a bridge is, what it costs, how it fails, and the
conditions under which it would be safe to ship.

---

## 1. What a bridge actually is (and is not)

Hikmalayer is a sovereign Layer-1 with its own consensus, its own state
machine, and no EVM. BTC lives on Bitcoin; ETH, USDT and USDC live on
Ethereum; SOL lives on Solana. **Those assets can never "move" to
Hikmalayer.** What a bridge does is:

1. **Lock** the real asset in a custody address on its native chain.
2. **Mint** a *wrapped claim* on Hikmalayer — an HTS token (Brick 1) such as
   `hkmBTC` — that represents "one BTC held in bridge custody".
3. **Burn** the wrapped claim on Hikmalayer and **release** the real asset on
   the native chain when the user exits.

Two consequences must be stated publicly and repeatedly:

- The wrapped token is **not** BTC. It is a *credit claim against the bridge's
  custody and honesty*. If custody is compromised, `hkmBTC` becomes worthless
  no matter how correct Hikmalayer's own consensus is.
- A bridge therefore **imports a new trust assumption** into an otherwise
  self-contained chain. Hikmalayer's sovereign-finality guarantees say nothing
  about whether Bitcoin custody is solvent.

## 2. Threat model — why bridges are the most-attacked component in crypto

Historic losses, all bridge-specific (not chain-consensus) failures:

| Incident | Loss | Root cause class |
|---|---|---|
| Ronin | ~$600M | Validator key compromise (5/9 signers) |
| Wormhole | ~$320M | Signature-verification bug (forged guardian set) |
| Nomad | ~$190M | Bad initialization: any message provable |
| Harmony Horizon | ~$100M | 2/5 multisig key compromise |

The pattern: **the bridge's signer set or its message verification is the
attack surface, not the underlying chains.** A bridge concentrates the value
of every deposit behind one trust mechanism, which makes it the single most
valuable target in the ecosystem.

Concrete threats to design against:

- **T1 Custody key compromise** — signer keys stolen or coerced; attacker
  releases locked assets.
- **T2 Forged deposit proof** — attacker convinces Hikmalayer a deposit
  happened that did not, and mints unbacked wrapped tokens.
- **T3 Reorg / finality mismatch** — a deposit is credited, then the source
  chain reorgs it away, leaving wrapped supply unbacked.
- **T4 Double-withdrawal / replay** — one burn redeemed twice on the source
  chain.
- **T5 Insider rug** — the operators themselves drain custody.
- **T6 Liveness failure** — signers go offline; user funds are locked
  indefinitely (not theft, but indistinguishable to the victim).
- **T7 Upgrade capture** — whoever can upgrade the bridge can steal from it.

## 3. Security models — the real choice

### Option A — Federated multisig (M-of-N operators)
A named set of operators observe both chains and co-sign mint/release.

- **Trust**: M-of-N honest and uncompromised. Explicitly custodial.
- **Effort**: lowest (weeks–months).
- **Failure**: T1/T5 are existential. This is exactly the Ronin/Harmony model.
- **Honest label required**: "federated custodial bridge", never
  "decentralized" or "trustless".

### Option B — MPC / threshold signatures (TSS)
As A, but no single machine ever holds a whole custody key; signing is a
distributed protocol.

- **Trust**: still M-of-N operators, but key *extraction* is much harder.
- **Effort**: high — TSS is subtle, and implementation bugs are silent.
- **Failure**: collusion still drains custody. Better against theft, not
  against dishonest operators.

### Option C — Light-client / proof verification (trust-minimized)
Hikmalayer verifies source-chain consensus proofs directly in its own
consensus (e.g. Bitcoin SPV headers + Merkle inclusion proofs).

- **Trust**: cryptographic and economic assumptions of the source chain only —
  no operator honesty for *minting*.
- **Effort**: very high (many engineer-months per source chain), and the
  *outbound* direction still needs custody unless the source chain can verify
  Hikmalayer (it cannot, without a contract on that chain).
- **Failure**: bugs in header/proof verification (Wormhole class), but no
  standing "keys that can steal everything".

**Recommendation.** If a bridge is built at all: start with **Option A or B for
a single asset**, label it honestly as custodial, cap it hard, and treat
Option C as a later upgrade for the highest-value asset. Do **not** attempt
"top 100 coins" — each source chain is its own integration, its own finality
rules, and its own audit.

## 4. What Hikmalayer itself would need (consensus work)

Wrapped assets ride on Brick 1 (HTS), which already gives fungible tokens,
balances in the state root, and DEX pools. The bridge-specific additions:

1. **Mint/burn authority bound to a bridge policy object.** Today HTS supply
   is fixed at creation — deliberately. A wrapped asset needs supply that
   moves with custody, so consensus must recognise a *bridge-controlled*
   token whose mint/burn requires an M-of-N attestation, with the signer set
   itself stored in the state root (so every node enforces it identically).
2. **Attestation transaction type** carrying source-chain txid, amount,
   recipient, and the operator signatures — with strict replay protection
   keyed by source txid (defeats T4).
3. **Confirmation-depth rule per source chain** before minting (e.g. Bitcoin
   ~6 blocks, Ethereum post-finality), enforced by consensus, not by an
   operator's judgement (defeats T3).
4. **Global and per-epoch mint caps** in consensus, so a total signer
   compromise is bounded by a rate limit rather than being unbounded (bounds
   T1/T5).
5. **Pause / circuit breaker**: a consensus-recognised halt of bridge
   mint/release that any quorum of validators (not just bridge operators) can
   trigger.
6. **Public proof-of-reserves endpoint**: wrapped supply on Hikmalayer vs.
   observed custody balance on the source chain, continuously published.

Off-chain, the bridge also needs relayer daemons per source chain, hardened
key management (HSM), and monitoring/alerting on the reserve invariant.

## 5. Phased delivery plan with hard gates

| Phase | Deliverable | Gate before proceeding |
|---|---|---|
| 0 | This document reviewed; go/no-go on being custodial | Written acceptance that v1 is custodial |
| 1 | Consensus additions (§4.1–4.6) + tests, no live custody | Full test suite; internal review |
| 2 | Testnet bridge, one asset, worthless testnet coins | ≥1 month adversarial testnet, no incidents |
| 3 | **External audit of bridge + consensus additions** | Audit passed and remediations verified |
| 4 | Mainnet, one asset, hard cap (e.g. equivalent of $50–100k) | Cap respected; proof-of-reserves public and green |
| 5 | Raise caps / add a second asset | Sustained clean operation; re-audit per asset |

**Launch-blocking criteria — do not go live if any is unmet:**
- No completed external audit of the bridge code.
- No public proof-of-reserves.
- No enforced mint cap and no circuit breaker.
- Custody keys not in HSMs, or any single person able to sign alone.
- Marketing that calls the bridge "trustless" or implies wrapped assets are
  the native assets.

## 6. Cost and operational reality

- **Engineering**: Option A single-asset ≈ 2–4 engineer-months; Option C for
  Bitcoin ≈ 6–12 engineer-months. Add per-asset integration cost.
- **Audit**: a bridge audit is priced well above a consensus audit because of
  the value at risk; budget for a dedicated engagement plus re-audits.
- **Operations**: 24/7 monitoring, key ceremonies, incident response, and
  legal/regulatory exposure — custodying other people's BTC is, in many
  jurisdictions, a regulated activity. **Take legal advice before Phase 4.**
- **Insurance/reserve**: consider a treasury-funded backstop sized to the cap.

## 7. Interim alternatives (no bridge required)

If the goal is "users can get value in and out", these are cheaper and
carry no custody risk to the protocol:

1. **Exchange listing** — a centralized exchange lists HKM and handles fiat
   and crypto on-ramps. The custody risk sits with the exchange, not with
   Hikmalayer's protocol.
2. **OTC / payment-processor on-ramp** for HKM purchases.
3. **Ecosystem-native stable unit** issued as an HTS token by a
   collateralized issuer, honestly labelled as issuer credit — useful as a
   DEX quote asset without touching another chain.

These let the ecosystem and its DEX grow while the bridge question stays
open.

## 8. Communication rules (binding on all materials)

Until a bridge exists and is audited:

- The DEX is described as **"for HKM and Hikmalayer-issued tokens"**, never
  "supports the top 100 coins".
- No page, chart, or listing may display BTC/ETH/USDT/SOL as tradeable on
  Hikmalayer.
- After a bridge ships, wrapped assets are always labelled `hkmBTC`-style with
  a visible "wrapped, redeemable via the Hikmalayer bridge" note and a link to
  live proof-of-reserves.
- Anyone publishing a "HKM contract address" on another chain is running a
  scam; state this in official channels.

---

## Decision taken: not pursuing a bridge

The project has chosen **not** to build a bridge. This avoids, in one stroke:
custody of other people's assets, the single largest attack surface in the
industry, a dedicated audit budget, 24/7 signer operations, and the legal
exposure of custodying regulated assets.

**Strategy instead:** grow ecosystem value on Bricks 1–2 (native tokens + the
AMM DEX), where Hikmalayer's own consensus is the only thing a user must
trust, and use the non-bridge on-ramps in §7 (exchange listing, OTC) if
external liquidity is ever needed.

Should anyone propose revisiting this, these five questions must be answered
in writing first — and a "no" to any of them ends the discussion:

1. Do we accept a **custodial** bridge for v1? (If no, budget for Option C.)
2. Which **single** asset first, and what **hard cap**?
3. Who are the **operators**, and what is the key-management standard?
4. Who **pays for the audit**, and when is it scheduled?
5. Have we taken **legal advice** on custodying that asset?
