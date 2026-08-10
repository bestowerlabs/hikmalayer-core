# Benchmark Report

**Status:** Execution-layer benchmark complete. Wide-area consensus benchmark
**not** performed — it needs a public testnet with independent operators.

Reproduce with `BENCHMARKING.md` and `ops/run_benchmark.sh`.

---

## 1. What was measured

**Environment:** Docker Compose — bootnode + validator nodes + RPC, with
Prometheus and Grafana. REST transaction harness. Single host.

**10-minute sustained run:**

| Metric | Result |
|---|---|
| Duration | 600 s |
| Total transactions | 8,940 |
| Average throughput | 14.88 TPS |
| Average latency | ~67 ms |
| Reorgs | 0 |
| Memory per node | ~4–5 MB |

**Observations:** continuous load sustained without crashes; all services stable
throughout; memory footprint flat.

Artifacts: `bench/results/run_10min/` — `benchmark_report.json`, `.csv`, `.md`.

## 2. Cryptographic operation costs

Measured single-core, the numbers that actually shape block size and validation
work:

| Operation | Classical | Quantum-ready |
|---|---|---|
| Key derivation | <1 ms | ~3 ms (ML-DSA-65 keygen) |
| Signing | <1 ms | ~11 ms |
| Verification | <1 ms | ~4 ms |
| Bytes on the wire | ~130 | ~5,400 |

A block full of hybrid transactions is roughly 40× larger than the classical
equivalent. That is a bandwidth and storage question long before it is a CPU
question, and it is the honest cost of `quantum_readiness.md`.

## 3. Scope — what these numbers do NOT show

Stated plainly, because a throughput figure quoted without its conditions is
worse than no figure:

- **Not a wide-area benchmark.** One host, container networking. Block
  propagation across real network paths is not measured.
- **No fork or partition behaviour.** Zero reorgs here means none occurred in a
  setting where they were unlikely, not that the fork-choice logic was exercised.
- **No validator-set scale.** A handful of local validators, not 50+ independent
  operators with real stake distribution.
- **Not a throughput ceiling.** Throughput is bounded by the per-block
  transaction cap (100) and block interval (15 s target), not by execution
  speed — execution is a state-machine step, not a VM. The measured figure
  reflects the harness and those bounds.

## 4. Original targets, and where they stand

| Target | Status |
|---|---|
| 1,000+ TPS sustained | **Not met, and not currently a goal.** It would require raising the per-block cap and block cadence, which raises fork rates. That trade should be made against public-testnet measurements, not local ones |
| Block finality < 5 s | **Not applicable as stated.** Finality here is depth-based with a validator-progress-first fork choice, not a fixed-time BFT commit. Block cadence targets 15 s |
| 50+ validator simulation | **Not done.** Needs the adversarial testnet |
| Horizontal node scaling | Partially — multi-node Compose deployment operational |
| Snapshot / fast sync | **Done.** Checkpoint fast-sync is implemented, self-verifying and boundary-anchored, reconstructing a byte-identical state root (automated equivalence test) |

## 5. Next

Wide-area measurement belongs to the public adversarial testnet, which is a
launch prerequisite (`mainnet_readiness.md`). Until then, no throughput or
finality claim about real network conditions should be made on the basis of this
report.
