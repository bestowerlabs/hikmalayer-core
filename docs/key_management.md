# Key Management

Every key Hikmalayer uses, where it belongs, and what happens if it is lost.

**One rule underpins the rest: the node never accepts a private key.** There is
no endpoint that takes one, for any purpose. Signing happens in a wallet, in
the browser extension, or in the offline `hikma-wallet` CLI, and only the
signature is submitted.

---

## 1. The keys

| Key | What it controls | Where it belongs |
|---|---|---|
| **Account private key** (32 bytes, secp256k1) | An `hkm…` account *and* the `hkq…` account derived from it — two different accounts with separate balances | Wallet extension for everyday use; offline CLI for anything large |
| **ML-DSA-65 key** | The post-quantum half of a `hkq…` account | **Derived** from the account key — not stored separately, not backed up separately |
| **VRF key** (sr25519) | A validator's leader-election identity | Derived from the account key; registered on chain when staking |
| **Node key** | A node's P2P identity (`node_id`) | Node environment |
| **Admin / P2P tokens** | Faucet, mining, difficulty, governance; peer endpoints | Node environment / secret manager. Bearer credentials, **not** signatures |

### One secret, several identities

Everything above except the node key and the bearer tokens is **derived from a
single 32-byte private key**:

```
private key ──┬─► secp256k1 public key ──► hkm… address
              ├─► sr25519 VRF key       ──► validator identity
              └─► ML-DSA-65 key (domain-separated seed)
                        └─ together with the secp256k1 key ──► hkq… address
```

So there is exactly one thing to back up, and it does not change when an
account becomes quantum-ready. Derivation is domain-separated, so the
post-quantum key is not the master secret in another form: learning one
scheme's key tells an attacker nothing about the other's.

```bash
hikma-wallet keygen                      # a new secret, and both identities
hikma-wallet identity <private_key_hex>  # both identities for a key you hold
```

**There is no seed phrase and no HD derivation.** Each account is an
independent key and each needs its own backup. This is a real usability cost
and it is stated rather than glossed.

---

## 2. Which key goes where

| Use | Where it belongs |
|---|---|
| Everyday spending, DEX trading | **Browser extension** — the key lives in the extension's own context, where a website (or an XSS in one) cannot read it |
| Convenience in a dashboard session | In-page wallet — hardened, but the key is in the page while unlocked |
| Treasury, genesis, large holdings | **Offline `hikma-wallet` CLI only**, on a machine that does not browse the web |
| Validator block signing | Node environment (`VALIDATOR_PRIVATE_KEY`), ideally an HSM or remote signer |

### The hybrid-validator constraint

A `hkq…` validator signs blocks with **both** schemes. Its ML-DSA key derives
from the same secret, so there is still one thing to protect — but a remote
signer or HSM has to be able to produce ML-DSA-65 signatures, and most cannot
today. Until that changes, a hybrid validator's signing key is a software key
on the node. That is a genuine limitation of deploying post-quantum signatures
in 2026, not a design choice, and it should factor into whether a given
validator runs classical or hybrid.

---

## 3. Storage at rest

**Browser wallet and extension:**

- Generated with the platform CSPRNG, in the browser. Never transmitted to a
  node, never placed in a URL, never logged.
- Persisted **only** as AES-256-GCM ciphertext under a key derived from the
  user's password with PBKDF2-SHA256 at 310,000 iterations (OWASP guidance).
  Without the password the stored blob is useless.
- While unlocked, the key is **not** held as a string. It is encrypted under a
  per-session, **non-extractable** WebCrypto key and decrypted into a
  short-lived buffer only for the instant of signing, then zeroized. JavaScript
  strings cannot be wiped, so the runtime path never creates one.
- Wiped on lock, on inactivity timeout (15 minutes), and on tab close.
- Export requires the password again.
- A multi-account keyring encrypts all keys together under one password, with
  public-only metadata alongside so the UI can list accounts while locked. The
  decrypted keys are checked against that metadata, so tampering with it cannot
  make the UI show — or sign for — the wrong address.

**Node:**

- `VALIDATOR_PRIVATE_KEY` from the environment. Use a secret manager; do not
  bake it into an image or a shell history.
- State is written atomically (temp file + rename), so a crash mid-write cannot
  corrupt it.

---

## 4. Rotation

**Bearer tokens** rotate without downtime:

```bash
ADMIN_TOKEN_CURRENT=<new>   ADMIN_TOKEN_PREVIOUS=<old>
P2P_TOKEN_CURRENT=<new>     P2P_TOKEN_PREVIOUS=<old>
```

Both values are accepted during the overlap, and comparison is constant-time.
Optionally, `mint_token` issues HMAC-signed, self-expiring, scope-bound tokens
verified statelessly and fail-closed.

**Validator keys** do not rotate in place: the address *is* the key. Rotating
means unbonding the old validator and staking a new one, which is a
governance-visible action by design — a key that could be swapped silently
would be a key an attacker could swap silently.

**Account keys** likewise. Moving to a new key means transferring the balance.

---

## 5. Loss and compromise

| Event | Consequence |
|---|---|
| Account key lost | Funds are unrecoverable. There is no recovery mechanism, and any that existed would be an attack surface |
| Account key stolen | The classical `hkm…` account is gone immediately; the `hkq…` account is gone too — hybrid defends against *cryptanalysis*, not theft |
| Password lost, ciphertext held | Unrecoverable. Export the key while you still can |
| Validator key stolen | The thief can sign blocks and unbond stake. Equivocation is slashable, which limits but does not undo the damage |
| Admin token leaked | The holder can drive faucet, mining, difficulty and governance endpoints. Rotate immediately |

**What hybrid does and does not do.** A `hkq…` account survives an attacker who
has broken secp256k1 and holds the *derived* classical private key, because
ML-DSA is still required. It does not survive an attacker who has the **master
secret**, because both keys derive from it. Hybrid is protection against a
future cryptanalytic break, not against key theft.

---

## 6. Operator checklist

- [ ] No key in a container image, a repository, or shell history
- [ ] `ADMIN_TOKEN` and `P2P_TOKEN` set — an unset token **disables** the
      endpoint rather than opening it, so verify the ones you need are set
- [ ] Treasury and genesis keys generated and kept offline
- [ ] `GENESIS_VALIDATOR_PQ_PUBLIC_KEY` set if the genesis treasury is a `hkq…`
      address — otherwise the chain refuses to seat it as a validator, which is
      loud but will halt block production
- [ ] Backups of every account key tested by restoring one, not assumed
- [ ] Rotation procedure exercised before it is needed
