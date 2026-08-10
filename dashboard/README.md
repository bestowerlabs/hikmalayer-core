# Hikmalayer Dashboard

The React front end for Hikmalayer: DEX, wallet, asset explorer, credentials and
chain explorer, talking to a node over the REST API.

## Running it

```bash
npm install
npm run dev                     # Vite dev server on :5173
```

Point it at a node — start one with `ops/devnet.sh` from the repository root.
The node must allow this origin:

```bash
CORS_ALLOWED_ORIGINS=http://localhost:5173 ./hikmalayer
```

`ops/devnet.sh` already permits the Vite dev and preview origins.

```bash
npm run build                   # production build
npm run lint
```

## What it does

| Panel | |
|---|---|
| **Wallet** | Create/import a key, unlock, view balance, switch between the key's **Classical** and **Quantum-ready** accounts |
| **Swap** | Live on-chain quotes, slippage → `min_out`, price impact |
| **Liquidity** | Pool reserves, spot price, LP position and share |
| **Assets** | Token registry, issuance, transfers, holdings |
| **Credentials** | Issue, revoke and verify Proof-of-Credential records |
| **Explorer / Mining** | Blocks, chain stats, validator set, admin actions |

## Signing

Three signers, in descending order of safety, selected automatically:

1. **Browser extension** (`extension/`) — the key lives outside the page, so an
   XSS here cannot reach it. Preferred whenever installed.
2. **In-page wallet** — AES-256-GCM vault under PBKDF2-SHA256 (310,000
   iterations). While unlocked, the key is held under a **non-extractable**
   WebCrypto key and decrypted to a short-lived buffer only for the instant of
   signing, then wiped. Auto-locks on idle and on tab close.
3. **Offline flow** — with no signer available, every panel shows the exact
   `hikma-wallet` command and canonical message and accepts a pasted signature.
   The right choice for cold keys.

Every signature requires approval of the exact canonical message. There is no
silent signing.

### Quantum-ready accounts

The wallet panel has a **Classical / Quantum-ready** switch. Quantum-ready
selects the key's `hkq…` account, which signs with ML-DSA-65 alongside ECDSA.

Two things to know: the two accounts hold **separate balances** — switching
moves no funds — and the hybrid identity is derived on unlock and held **in
memory only**, so a locked wallet reports no `hkq…` address rather than quietly
falling back to the classical one.

The paste-a-signature flow is classical-only: an ML-DSA signature is ~6.6 KB of
hex, which is not something to move through a textarea.

See [`docs/wallet_security.md`](../docs/wallet_security.md).

## Security notes

A strict CSP (`script-src 'self'`, `connect-src` pinned to the node) is enforced
and browser-verified to block injected scripts and exfiltration. Attacker-chosen
token names and symbols are stripped of invisible and bidirectional characters
and flagged when they imitate HKM.

**Residual risk:** script running in this page can still *ask* the in-page
wallet to sign — the confirmation makes that visible, not impossible. That is
exactly why the extension exists and is preferred, and why treasury and
validator keys belong offline.

## Licence

HikmaLayer Business Source License 1.1 — see the repository `LICENSE`.
