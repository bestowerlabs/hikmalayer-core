# Local Blockchain Testing Report

> **Historical record.** This is a dated snapshot from the time it was written,
> retained as provenance. It does **not** describe the current tree — the suite
> has since grown to 139 unit tests plus 40 adversarial tests, alongside 58 SDK
> offline tests, 21 live integration tests and a 16-check end-to-end
> application. For current verification steps see the repository `README.md`
> and `docs/security_assessment.md`.

Date: 14 July 2026
Environment: Windows 11 + WSL2 (Ubuntu 26.04), Rust toolchain installed in-WSL

## Method
Ran the full Rust test suite locally using `cargo test -j 2` (parallel job count reduced to fit an 8GB RAM machine) against the current main branch.

## Results
- Total tests run: 70
- Passed: 70
- Failed: 0
- Ignored: 0
- Duration: 1.01s

## Coverage Highlights
- Blockchain core: block creation, tampering detection, fork-choice, difficulty retargeting
- Consensus: Proof-of-Work, Proof-of-Stake selection, VRF randomness beacon
- Security-relevant: P2P token rejection/replay protection, forged/unsigned block rejection, admin/treasury key requirements on faucet, credential lifecycle
- Networking: peer scoring, banning on repeated misbehavior, envelope replay protection

## Notes
- A separate `hikma-wallet` unit-test binary also ran as part of the same `cargo test` invocation; confirm its pass/fail summary line before treating the full suite as fully green.
- No flaky or skipped tests observed in the primary 70-test suite.
- On memory-constrained machines (e.g. 8GB RAM), `cargo test` with default parallelism can fail with `Cannot allocate memory (os error 12)`. Passing `-j 2` (or `-j 1`) resolves this at the cost of longer compile time.
