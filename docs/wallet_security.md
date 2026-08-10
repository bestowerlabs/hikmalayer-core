# Hikmalayer Wallet — Security Model

This describes exactly what the in-browser wallet protects against, what it
does not, and which key belongs in which place. It is deliberately blunt: a
wallet that oversells its guarantees gets people robbed.

## 1. What the wallet is

A key generated in your browser, persisted **only** as AES-256-GCM ciphertext
under a password stretched with PBKDF2-SHA256 (310,000 iterations). Signing
happens locally; the node only ever receives a public key and a signature.

Signing reproduces `src/consensus/pos.rs` exactly:

```
address   = "hkm" + hex(SHA256(uncompressed secp256k1 pubkey)[..20])
message   = "<chain_id>:<canonical message>"
digest    = SHA256("\x19Hikmalayer Signed Message:\n" + <UTF-8 byte length> + message)
signature = hex(compact ECDSA r||s), low-S normalized
```

The `chain_id` prefix is what stops a signature made on one network being
replayed on another — addresses come from the key, so the same account exists
on a testnet and on mainnet. It is a *visible* prefix rather than a hidden
change to the digest precisely so the confirmation dialog can show the user
which network they are authorizing.

Signatures are **byte-for-byte identical** to those from the `hikma-wallet`
CLI — verified across transfer, swap and token-create domains, including a
UTF-8 case that exercises byte-length (not character-length) digests.

## 2. Threats, and what stops them

| Threat | Mitigation | Status |
|---|---|---|
| Key stolen from disk / another site reads it | Stored only as AES-GCM ciphertext; `localStorage` is origin-scoped; the vault carries only public metadata | Mitigated |
| Weak password brute-forced | PBKDF2-SHA256, 310k iterations, random 16-byte salt | Mitigated (choose a strong password) |
| Tampered vault (swapped address) | Decryption re-derives the address and refuses a mismatch | Mitigated |
| Injected script executes (XSS) | CSP `script-src 'self'`; no `dangerouslySetInnerHTML`, no `eval`, no wallet globals on `window` | Mitigated |
| Stolen data exfiltrated to attacker host | CSP `connect-src` pinned to self + the node; `object-src`/`base-uri`/`form-action` locked | Mitigated |
| Passive scraping of key from memory/state | Key held encrypted under a **non-extractable** session key; decrypted to a buffer only during signing; signing takes raw bytes so no key string exists; buffer zeroized after | Mitigated |
| Silent background signing | Every signature requires explicit approval of the exact canonical message | Mitigated |
| Asset-identity spoofing (bidi/zero-width/"HKM" imitation) | Untrusted chain strings stripped, bounded, and flagged in the UI | Mitigated |
| Unattended unlocked session | Auto-lock after 15 minutes idle; wipe on tab close; export re-prompts for the password | Mitigated |
| **Active script asks the wallet to sign** | Confirmation dialog makes it visible and refusable — **but a user who approves is still signing** | **Residual** |
| Compromised device (malware, keylogger) | Out of scope for any software wallet | **Residual** |
| Malicious/compromised dependency in the build | Pinned deps; CSP limits blast radius; audit before release | **Partly residual** |

## 2b. The browser EXTENSION removes the main residual risk

The row marked **Residual** above — "active script asks the wallet to sign" —
is the reason the extension in [`extension/`](../extension/README.md) exists.
There, the key lives in the extension's own context:

| Property | In-page wallet | **Extension** |
|---|---|---|
| Key readable by page script | While unlocked, yes | **No — different context entirely** |
| Approval UI drawable by the page | It is *in* the page | **No — extension window** |
| Site permissions | n/a | **Per-origin connect, browser-supplied origin** |
| Page can call privileged methods | n/a | **No — method allowlist in the relay** |

Verified in a real browser: the page cannot reach `chrome.storage`, cannot
invoke `wallet:export`, cannot approve its own request, and sees no accounts
until connected — yet an approved signature is accepted by a live node.

The extension holds **multiple accounts in one keyring** under a single
password. Three properties are worth stating plainly, because each closes a
gap that a naive implementation leaves open:

- **A signature is pinned to the account the approval displayed.** Switching
  accounts while an approval window is open cannot redirect the signature.
- **A site only learns about accounts it is connected to.** Switching accounts
  emits `accountsChanged` to connected origins only — an unconnected page
  sitting in a background tab learns nothing.
- **An approval window belongs to its request.** Closing some other browser
  window does not cancel an approval you are still reading; only closing that
  approval rejects it.

There is no seed phrase and no HD derivation: each account is an independent
key, so **each one needs its own backup**.

**Recommendation:** install the extension and use it as the signer. The
dashboard detects it automatically and prefers it; the in-page wallet remains
for users who have not installed it.

## 3. Which key goes where

| Key | Where it belongs |
|---|---|
| Everyday spending / DEX trading | **Extension wallet** (preferred), or the in-page wallet |
| Treasury, genesis, large holdings | **Offline `hikma-wallet` CLI only.** Sign on a machine that does not browse the web; the dashboard's offline paste flow submits the signature |
| Validator block-signing key | Node environment (`VALIDATOR_PRIVATE_KEY`), ideally an HSM or remote signer — see `docs/key_management.md` |

The dashboard keeps the **offline signing flow** available at all times for
exactly this reason: with the wallet locked, every DEX panel shows the precise
`hikma-wallet` command and canonical message, and accepts a pasted signature.

### Classical or quantum-ready

Both the in-page wallet and the extension expose a **Classical /
Quantum-ready** switch. Quantum-ready selects the key's `hkq…` account, which
signs with ML-DSA-65 alongside ECDSA and therefore survives a quantum break of
secp256k1 — see [quantum_readiness.md](quantum_readiness.md).

Two consequences worth stating to users:

* The two accounts have **separate balances**. Switching does not move funds.
* The hybrid identity is derived on unlock and held **in memory only**. Nothing
  extra is written to disk, and a locked wallet does not report a `hkq…`
  address — a site is told there is no account rather than being quietly
  handed the classical one, which would have it paying the wrong address.

The dashboard's paste-a-signature flow remains classical-only: an ML-DSA
signature is ~6.6 KB of hex, which is not something to move through a textarea.
Use the wallet or the extension for hybrid accounts.

## 4. Deployment requirements

1. **Serve over HTTPS** (or localhost). The wallet refuses to operate in an
   insecure context — WebCrypto is unavailable there.
2. **Set `VITE_API_BASE`** to your node's origin before building (see
   `dashboard/.env.example`). The build bakes it in *and* adds it to the
   page's `connect-src`, so the dashboard is never blocked by its own policy.
   To retarget an already-built bundle, serve an inline script setting
   `window.__HIKMALAYER_NODE__` — it takes precedence.
3. **Send the CSP as an HTTP header**, not only the `<meta>` tag in
   `dashboard/index.html`. The meta tag cannot enforce `frame-ancestors`
   (browsers ignore it there), and that directive is what blocks clickjacking
   of the confirmation dialog — so the header is not optional:
   ```
   Content-Security-Policy: default-src 'self'; script-src 'self';
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:;
     connect-src 'self' https://<your-node-host>; object-src 'none';
     base-uri 'none'; form-action 'none'; frame-ancestors 'none'
   ```
4. **Set `CORS_ALLOWED_ORIGINS`** on the node to the dashboard's origin — it is
   an explicit allowlist, never a wildcard.
5. **Add `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, and
   HSTS** at the web server.
6. **Review dependencies before each release** (`npm audit`); the wallet's
   crypto comes from `@noble/curves` and `@noble/hashes`, which are audited and
   dependency-free.

## 5. What users should be told

- Back up the private key shown at creation; it cannot be recovered.
- The password protects the key on this device — losing it loses the wallet.
- Read the confirmation dialog. It shows the exact message being authorized;
  if it does not match what you just did, reject it.
- A token can call itself anything, including "HKM". Only the native coin is
  HKM; tokens flagged in red are imitating it.
- Nobody legitimate will ever ask for your private key.
