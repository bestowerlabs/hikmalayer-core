# Benchmarking Hikmalayer

Benchmark commands for local testnet runs using Docker Compose and the harness
in `bench/`.

**Read the scope limits first.** These measure the execution and API layer on a
single host. They are **not** a wide-area consensus benchmark: block propagation
across real network paths, fork resolution under partition, and validator gossip
at scale are all unmeasured, and need a public testnet with independent
operators. Results and their limits: [`docs/benchmark_report.md`](docs/benchmark_report.md).

## Prerequisites

- Docker + Docker Compose
- Python 3

## Start the testnet

```bash
./ops/start_testnet.sh
```

## Quick 10‑minute run

```bash
./ops/run_benchmark.sh 600
```

## 1‑hour run

```bash
./ops/run_benchmark.sh 3600
```

## 72‑hour endurance run

```bash
./ops/run_benchmark.sh 259200
```

## Stop or reset the testnet

```bash
./ops/stop_testnet.sh
```

```bash
./ops/reset_chain.sh
```

Benchmark outputs are saved under `bench/results/`:

- `benchmark_report.json`
- `benchmark_report.csv`
- `benchmark_report.md`
