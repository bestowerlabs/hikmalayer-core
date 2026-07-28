# Hikmalayer Wallet — Browser Extension

A MetaMask-style wallet for Hikmalayer. The private key lives in the
extension's own context, so **a website — or an XSS in a website — cannot read
it**. Sites may ask to connect and ask for signatures; you approve or reject
each one in extension UI that no page can draw over or click for you.

## Why it exists

The in-page wallet in `dashboard/` is convenient and hardened, but its key is
in the page while unlocked: script running there can still *request* a
signature. Moving the key into an extension removes that class of attack —
the page has no path to the key at all, only a message channel that the
extension arbitrates.

## Build and install

```bash
cd extension
npm install
npm run build          # → extension/dist
```

Then in Chrome/Edge/Brave: `chrome://extensions` → enable **Developer mode**
→ **Load unpacked** → select `extension/dist`.

Output is deliberately **unminified**: a wallet should be reviewable by the
people trusting it.

## Architecture

```
web page                content script            background service worker
(MAIN world)            (ISOLATED world)          (extension context)
────────────            ────────────────          ─────────────────────────
window.hikmalayer  ──►  method allowlist    ──►   vault (chrome.storage)
  .connect()            relay only                unlocked key (session-encrypted)
  .signMessage()        no keys, no decisions     approval queue
       ▲                                          signing
       └──────────── approved result ─────────────┘
                                                   ▲
                                   popup.html (approve / reject)
```

- `src/inpage.js` — the provider a dapp sees. Holds no secrets; it is frozen
  so a hostile script cannot replace it with a phishing lookalike.
- `src/content.js` — isolated relay. Enforces a method allowlist; the origin
  used for permissions comes from the browser (`sender.origin`), never from
  the page, so a site cannot impersonate another.
- `src/background.js` — the security boundary. Vault, permissions, approval
  queue, and signing all live here.
- `src/popup.js` + `public/popup.html` — onboarding, unlock, accounts,
  connected sites, and the approval screens.

## Provider API

```js
if (window.hikmalayer?.isHikmalayerWallet) {
  const [address] = await window.hikmalayer.connect();       // prompts once
  const publicKey = await window.hikmalayer.getPublicKey();
  const { signature } = await window.hikmalayer.signMessage(
    `hikmalayer-transfer:${address}:${to}:${amount}:${nonce}`
  );                                                          // prompts each time
}
```

| Method | Prompts? | Notes |
|---|---|---|
| `hikma_chainInfo` | no | name, ticker, decimals, address prefix |
| `hikma_accounts` | no | `[]` until the site is connected |
| `hikma_requestAccounts` | first time | returns the address |
| `hikma_getPublicKey` | no | connected sites only |
| `hikma_signMessage` | **every time** | shows the exact canonical message |

Events: `lock`, `accountsChanged` via `provider.on(...)`.

## Security properties (verified in a real browser)

- The page cannot reach `chrome.storage`, cannot call privileged methods
  (`wallet:export` and friends are not on the page allowlist), and cannot
  approve its own requests.
- An unconnected site sees no accounts.
- Every signature opens an extension approval showing the exact message;
  rejecting returns an error and produces no signature.
- The key is stored only as AES-256-GCM ciphertext (PBKDF2-SHA256, 310,000
  iterations). While unlocked it is held encrypted under a **non-extractable**
  session key and decrypted only for the instant of signing, then zeroized.
- Auto-locks after 15 minutes idle.
- Extension pages run under `script-src 'self'`.

## Signing scheme

Identical to `src/consensus/pos.rs` and the `hikma-wallet` CLI — signatures
are byte-for-byte the same:

```
address   = "hkm" + hex(SHA256(uncompressed secp256k1 pubkey)[..20])
digest    = SHA256("\x19Hikmalayer Signed Message:\n" + <UTF-8 byte length> + message)
signature = hex(compact ECDSA r||s), low-S normalized
```

The crypto is imported from `dashboard/src/lib/wallet.js` — one implementation,
one place to audit, no drift between the wallet, the dashboard, and the CLI.

## Limits

- Single account per install (multi-account is a natural next step).
- Talks to a node at `http://127.0.0.1:3000` for the balance display only;
  signing never needs a node. Update `host_permissions` in the manifest and
  the popup's fetch URL to point at a public node.
- Not published to any store; install unpacked, or package and sign it
  yourself. Before publishing, get the extension reviewed — it is the piece
  users trust with their keys.
