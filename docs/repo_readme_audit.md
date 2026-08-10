# Documentation Consistency Audit

Verifies that the documentation set describes the code as it actually is. Run
this check whenever the protocol changes; documentation drift is how a project
starts lying to its users without anyone deciding to.

**Scope:** `README.md`, `docs/*.md`, `sdk/README.md`, `extension/README.md`,
`ops/README.md`, checked against `src/`, `tests/` and the live API.

---

## Method

For each claim in the documentation, one of three verdicts:

- **Verified** — traceable to code or to a reproducible command
- **Corrected** — the claim was wrong and the document was fixed
- **Removed** — the claim could not be substantiated and did not belong

---

## Findings

### Corrected

| Claim | Where | Why it was wrong |
|---|---|---|
| "Smart Contract Framework — flexible contract execution environment" | Whitepaper §1.3, §4 | **There is no VM.** `ContractExecutor` is a credential registry. Replaced with §4, which explains the absence and its trade-offs |
| "Digital Signatures: future implementation" | Whitepaper §7 | Signatures shipped long ago, and now include a post-quantum half |
| "Transaction Merkle Roots: future enhancement" | Whitepaper §7 | Implemented |
| "Storage: all data stored in memory (lost on restart)" | Whitepaper §8 | State is persistent, written atomically, and rebuilt by replay |
| "No authentication required"; "no rate limiting implemented" | Whitepaper §11.2 | Value calls are signature-authorized and admin endpoints are deny-by-default. Rate limiting genuinely is absent, and now says so precisely |
| "Cross-Chain Bridges" on the roadmap | Whitepaper §9 | Directly contradicted the no-bridge decision **in the same document** |
| "Metacation Token (MCT), mintable with administrative controls" | Whitepaper §5 | HKM has a fixed emission schedule; HTS tokens have no mint function at all |
| GDPR "Right to Erasure: mechanisms for removing personal data" | Whitepaper §12.2 | A blockchain cannot delete history. Replaced with the accurate answer: no personal data goes on chain, so there is nothing to erase, and revocation is the operative remedy |
| "Actively participates in IEEE/ISO/W3C standards development"; ISO 27001 alignment | Whitepaper §12.3 | Unsubstantiated. Replaced with a split between standards actually conformed to in code (FIPS 204, FIPS 180-4, SEC 1/2, NIST SP 800-38D/132, OpenAPI 3.1) and formats merely compatible in principle |
| Two separate conclusions (§10 and §14) | Whitepaper | Duplicated and stale. Merged into one |
| Address regex `^hkm[0-9a-f]{40}$` | OpenAPI, SDK, dashboard, extension | Excluded `hkq…` accounts, making them unpayable from those clients |
| "Total tests run: 70" | README, mainnet_readiness | Now 139 unit + 40 adversarial + 58 SDK offline + 21 live |

### Removed

| Content | Why |
|---|---|
| Phase-4 benchmark, stated three times in `README.md` | One statement, in `docs/benchmark_report.md`, with its scope limits |
| "Phase-5 Roadmap (Public Testnet)" listing P2P consensus, fork choice, finality, signed handshakes, slashing and replay protection as *planned* | All implemented. A roadmap of completed work is misinformation |
| 18-row phase-status table | Development history, not user-facing documentation |
| "Remaining before public mainnet: VRF leader election, signed peer handshakes, fee market, difficulty retargeting" | All four are implemented. The real remaining item is the external audit |
| "Security Status (Sprint 1 Remediation)" | Superseded by `security_assessment.md` |
| Empty "Translations" placeholder | No translations exist |
| Stale project-directory tree | Omitted `sdk/`, `extension/`, `examples/`, `tests/` and most of `docs/` |

### Verified

- Consensus flow, fork choice, slashing and unbonding match
  `src/blockchain/chain.rs` and `src/blockchain/state.rs`
- Economic parameters match the constants in `src/blockchain/{transaction,state}.rs`
- Every endpoint in `docs/openapi.yaml` exists in `src/api/routes.rs`; the spec
  lints clean
- Cryptographic domain separators match the constants in `src/consensus/`
- The absence of a bridge is consistent across every document
- Test counts match a full run

---

## Standing rule

Two claim types need a citation to code or a runnable command, every time:

1. **Anything described as implemented.** If it cannot be traced, it is a plan
   and must be written as one.
2. **Any security or compliance property.** These are the claims a reader cannot
   check cheaply and will most reasonably rely on.

A documented limitation costs a paragraph. An undocumented one costs trust.
