# R-05 Integration Notes — HMAC-signed Token Rotation

## 1. Cargo.toml additions

Add to `hikmalayer-core`'s `[dependencies]` (pin exact versions to whatever
`cargo add` resolves at integration time):

```toml
hmac = "0.12"
sha2 = "0.10"
subtle = "2.5"
hex = "0.4"
base64 = "0.21"
clap = { version = "4", features = ["derive"] }
```

`clap` is only needed by the `mint_token` binary target, so it can go under
a `[[bin]]`-scoped dependency or stay in main deps — whichever matches the
existing project convention.

## 2. Wiring into state.rs

Replace:

```rust
admin_token: Option<String>,
p2p_token: Option<String>,
```

with:

```rust
admin_token_signing_key: Option<Vec<u8>>,
p2p_token_signing_key: Option<Vec<u8>>,
```

populated at startup via:

```rust
admin_token_signing_key: token::signing_key_from_env("ADMIN_TOKEN_SIGNING_KEY"),
p2p_token_signing_key: token::signing_key_from_env("P2P_TOKEN_SIGNING_KEY"),
```

## 3. Wiring into routes.rs

Replace the current `authorize_admin` / `authorize_p2p` calls with the
versions in `src/auth/mod.rs`. Call sites for the six HM-02 handlers
(`issue_certificate`, `transfer_tokens`, `mine_block`,
`set_mining_difficulty`, `stake_tokens`, `withdraw_stake`) don't need to
change shape — same `HeaderMap` + early-return pattern, just pointing at
the new signing-key field instead of the old static token field.

## 4. Env / deployment files

In `.env.example` and the production deployment runbook, replace:

```
ADMIN_TOKEN=
P2P_TOKEN=
```

with:

```
ADMIN_TOKEN_SIGNING_KEY=   # 64-char hex, e.g. `openssl rand -hex 32`
P2P_TOKEN_SIGNING_KEY=     # 64-char hex, e.g. `openssl rand -hex 32`
```

## 5. Rotation procedure (operational reference)

**Routine rotation:**

```bash
ADMIN_TOKEN_SIGNING_KEY=<key> cargo run --bin mint_token -- --scope admin --ttl 86400
P2P_TOKEN_SIGNING_KEY=<key>   cargo run --bin mint_token -- --scope p2p   --ttl 2592000
```

**Emergency revocation** (suspected leak): generate a new signing key
(`openssl rand -hex 32`), update the env var, restart. This invalidates
every outstanding token for that scope immediately.

**Recommended TTLs:** 8–24h for admin scope; 7–30 days for P2P scope.

## 6. Open items still pending Ayan's confirmation

- Whether signing keys live as plain env vars (current convention) or move
  to a secrets manager, given the EC2/Docker Compose deployment.
- Final target TTLs for each scope in this deployment.
- This closes R-05 in isolation; HM-01, HM-03, HM-06, HM-07 remain open.
