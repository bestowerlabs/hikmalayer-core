# Security Hardening Checklist

**Status:** Current. Reflects the code as it stands.

Companion documents: `security_assessment.md` (the 13 findings and their fixes),
`threat_model.md` (adversaries), `quantum_readiness.md` (the hybrid scheme),
`key_management.md`, `deployment_guide.md`.

---

## Cryptography

- [x] Native signing domain with per-operation message prefixes
- [x] Network-scoped signatures (chain id inside the signed bytes), enforced on
      both the submission and block-validation paths
- [x] Low-S normalized ECDSA; high-S signatures rejected
- [x] **One canonical encoding per public key** — closes transaction malleability
- [x] **Post-quantum hybrid accounts** (ML-DSA-65, FIPS 204) across transfers,
      tokens, AMM, vesting, credentials, staking, unbonding and block production
- [x] Address commits to both public keys, so neither can be substituted
- [x] Downgrade refused in both directions (hybrid without PQ fields; classical
      with them)
- [x] Domain separation between block and account signatures in **both** schemes
- [x] Deterministic ML-DSA keys and signatures, byte-identical across Rust and JS
- [ ] Post-quantum leader election — the sr25519 VRF is still classical
      (no standardized PQ VRF exists; see `quantum_readiness.md` §4)

## Validator / node authentication

- [x] The node never accepts a foreign private key, on any endpoint
- [x] Block signature must match the validator's key **registered on chain**
- [x] Hybrid validators verified against their on-chain ML-DSA key for both
      unbonding and block production
- [x] A hybrid genesis validator without a matching ML-DSA key is refused
      seating rather than seated with ECDSA-only protection
- [x] Permissioned validator onboarding available (`GENESIS_VALIDATOR_ALLOWLIST`,
      baked into the genesis state root)
- [x] Token rotation procedures implemented (`*_TOKEN_CURRENT`/`*_TOKEN_PREVIOUS`,
      constant-time comparison) and exercised
- [ ] HSM / vault-backed validator keys — an operator deployment choice.
      **Blocked for hybrid validators:** most HSMs cannot produce ML-DSA-65
      signatures today

## RPC & API protection

- [x] Deny-by-default admin and P2P tokens — unset **disables** the endpoint
- [x] Constant-time token comparison
- [x] Optional HMAC-signed, self-expiring, scope-bound tokens (`mint_token`)
- [x] Request body cap (1 MiB), mempool cap (1,000), per-block cap (100)
- [x] Mempool admission checks against projected state, so an inapplicable
      transaction costs one verification rather than a pool scan
- [x] Explorer and token-metadata inputs length-bounded and sanitized
- [x] Configurable CORS (`CORS_ALLOWED_ORIGINS`)
- [ ] **Per-IP rate limiting — NOT implemented.** Deploy behind a reverse proxy
      or WAF that provides it
- [x] No debug endpoints in the shipped surface

## P2P hardening

- [x] Signed gossip envelopes bound to the sender's derived `node_id`
- [x] `P2P_REQUIRE_IDENTITY=true` rejects unsigned envelopes
- [x] Peer reputation scoring with automatic banning of repeat offenders
- [x] Optional `P2P_ALLOWLIST` restricting participation to named node ids
- [x] Bounded message-id cache for replay protection
- [ ] Eclipse resistance depends on peer diversity — an operational property,
      not a protocol guarantee

## State and consensus integrity

- [x] Per-block state root, verified by re-execution on every node
- [x] Merkle-root transaction commitment
- [x] Checked arithmetic throughout; `overflow-checks` on in release
- [x] Recipient addresses validated before any credit
- [x] Authorization verified where state changes, not where callers remember
- [x] Adopted chains re-executed under **local** genesis parameters
- [x] Finalized history protected; validator-progress-first fork choice
- [x] Timestamps consensus-constrained in both directions
- [x] Difficulty clamped 1–5; retargeting deterministic and consensus-validated
- [x] Equivocation slashing, bounded by a slashing window equal to unbonding

## Secrets management

- [x] Tokens supplied via environment variables
- [x] No secrets in the repository, images or shell history (documented)
- [x] Atomic state persistence (temp file + rename)
- [ ] Vault-backed secrets in production — operator choice

## Wallet and client

- [x] Keys generated client-side; never transmitted, never logged, never in a URL
- [x] AES-256-GCM at rest under PBKDF2-SHA256 (310,000 iterations)
- [x] Unlocked key held under a non-extractable WebCrypto key; signing runs on
      raw bytes and zeroizes after
- [x] Mandatory per-signature confirmation of the exact canonical message
- [x] Strict CSP, browser-verified to block injected scripts and exfiltration
- [x] Anti-spoofing for attacker-chosen token names and symbols
- [x] Keys out of the page entirely via the MV3 extension
- [ ] Extension not published or externally reviewed — it is the component users
      trust with their keys
- [ ] No seed phrase / HD derivation; each account key needs its own backup

## Verification

- [x] 139 unit tests; 40 adversarial tests, run in debug **and** release
- [x] Every security finding has a regression test that fails pre-fix
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] Rust↔JS byte-parity asserted against the real signer
- [x] 21 live integration tests plus a 16-check end-to-end application
- [x] OpenAPI 3.1 description, lints clean
- [ ] Static analysis beyond clippy; dependency vulnerability scanning in CI
- [ ] Fuzzing of the transaction and envelope parsers

## The remaining gate

- [ ] **Independent external security audit.** Not performed. Everything above
      was found and fixed by the people who wrote the code. Post-quantum
      expertise must be in scope — see `external_audit_guide.md`
- [ ] **Public adversarial testnet** with independent validators
- [ ] Bug bounty, security contact process, incident-response retainer
