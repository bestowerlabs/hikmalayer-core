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
message   = "<chain_id>:<canonical message>"
digest    = SHA256("\x19Hikmalayer Signed Message:\n" + <UTF-8 byte length> + message)
signature = hex(compact ECDSA r||s), low-S normalized
```

The extension signs the message it is handed; the dapp builds it with the
network prefix. That prefix is visible in the approval screen, so a user can
see which network a signature is for — which is the point of putting it in
the message rather than hiding it in the digest.

The crypto is imported from `dashboard/src/lib/{wallet,hybrid}.js` — one
implementation, one place to audit, no drift between the wallet, the dashboard,
the SDK and the CLI.

## Quantum-ready accounts

The popup has a **Classical / Quantum-ready** switch. Quantum-ready selects the
key's `hkq…` account, authorized by **two** signatures over the same message:

```
address     = "hkq" + hex(SHA256("hikmalayer-hybrid-address-v1"
                                 ‖ secp256k1 pubkey ‖ ML-DSA-65 pubkey)[..20])
signature   = ECDSA, as above
pqSignature = ML-DSA-65 (FIPS 204), same message, context string "hikmalayer"
```

Both are required, so forging one transaction means breaking both schemes.
`hikma_signMessage` returns `pqSignature` and `pqPublicKey` alongside the
classical pair when — and only when — the active account is hybrid; a caller
that sees one must send both, because the node checks that the pair derives to
the sending address.

Three things worth knowing:

- **The two accounts hold separate balances.** Switching moves no funds; it
  changes which account sites are talking to, and connected sites are notified
  as they would be for any account change.
- **The hybrid identity is derived on unlock and held in memory only** (~3 ms of
  ML-DSA keygen per account). Nothing extra is written to storage. A locked
  wallet in quantum-ready mode reports **no** address rather than quietly
  falling back to the classical one — which would have a site paying the wrong
  account.
- **It costs more.** ~5.4 KB per transaction against ~130 bytes, and ~11 ms to
  sign instead of well under one.

`hikma_chainInfo` reports the active `scheme` and both address prefixes, so a
dapp can tell whether to expect post-quantum fields. See
[`docs/quantum_readiness.md`](../docs/quantum_readiness.md).

## Accounts

Several accounts live in one keyring, encrypted together under a single
password — one password to remember, one KDF run to unlock. Public metadata
(address, label) sits alongside the ciphertext so the popup can list accounts
while locked.

- **Add** generates a new key, or imports one you paste. A generated key is
  displayed once so you can back it up.
- **Switch** changes the account sites see; connected sites receive an
  `accountsChanged` event. Sites you have *not* connected are told nothing.
- **Rename** touches only the label — no password, no re-encryption.
- **Remove** re-encrypts the remaining keys, so it needs your password. The
  last account cannot be removed; remove the wallet instead.

A signature request is pinned to the account it displays: switching accounts
while an approval is open cannot redirect the signature to a different key.

## Network

The node URL is editable in the popup (**Network**). It is used only to show
your balance — signing never contacts a node. Only `http://` and `https://`
URLs are accepted.

## Limits

- Up to 20 accounts per wallet, all under one password. There is no seed
  phrase or HD derivation: each account is an independent key, so back up
  each one.
- Not published to any store; install unpacked, or package and sign it
  yourself. Before publishing, get the extension reviewed — it is the piece
  users trust with their keys.
