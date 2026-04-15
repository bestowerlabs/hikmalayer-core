# Hikmalayer Core

## What is Hikmalayer core?
Hikmalayer Core is a hybrid Layer‑1 blockchain prototype that combines Proof‑of‑Stake (validator
selection) with Proof‑of‑Work (block finalization). It provides:

- PoS validator selection, staking, and validator signature verification.
- PoW mining and PoW validation for every block.
- Governance and slashing configuration for validator accountability.
- Persistence of chain state to disk for safe restarts.
- P2P gossip and peer discovery for block propagation.
- A React dashboard for local interaction and testing workflows.

Hikmalayer is developed by Muhammad Ayan Rao, Founder and Director of Bestower Labs Limited.

This repository represents a production-focused hybrid L1 foundation implementing core consensus 
mechanics and operational services for future industrial-grade deployments.

For the official whitepaper, see `docs/Whitepaper.md`.

### Phase-4 Local Benchmark Results (API Execution Layer)
A Phase-4 local benchmark was conducted using a multi-container Docker Compose deployment (bootnode + validators + RPC + Prometheus + Grafana) to validate transaction execution throughput and operational stability.

**Environment:**

- Windows host
- Docker Compose multi-service deployment
- REST API transaction harness
- Prometheus + Grafana monitoring enabled

**10-Minute Sustained Run:**

- Duration: 600 seconds
- Total Transactions: 8,940
- Average Throughput: 14.88 TPS
- Average Latency: ~67 ms
- Reorg Count: 0 (instrumentation pending)
- Average Memory Per Node: ~4–5 MB

**Observations:**

- Continuous transaction load sustained without crashes.
- All services remained stable throughout the run.
- Extremely low memory footprint across all nodes.
- No chain reorganizations observed.
- Block production and finalized height are not yet included in this benchmark, as Phase-4 currently focuses on REST/API execution throughput rather than full P2P consensus orchestration.

**Scope Clarification:**

This benchmark measures transaction execution performance at the REST/API layer using a local multi-container deployment.

Full peer-to-peer consensus benchmarking (validator gossip, block finalization, fork handling, and genesis bootstrapping) is scheduled for Phase-5 (public testnet).

**Phase-4 Status:**

- Multi-node containerized environment operational
- Monitoring stack active (Prometheus + Grafana)
- Benchmark harness validated
- Sustained load test completed successfully

## Phase-4 Engineering Milestone: COMPLETE

- Benchmark artifacts are available under:

```bash
bench/results/run_10min/
```


## Licence
Hikmalayer licensing is split between source code, contributions, and documentation:

- **HikmaLayer Business Source License 1.1** for the protocol source code (see [`LICENSE`](LICENSE)).
- **HikmaLayer Contributor License Agreement (CLA)** for incoming contributions (see [`CLA.md`](CLA.md)).
- **Whitepaper** is released under **Creative Commons Attribution 4.0 International (CC BY 4.0)** to
  allow broad redistribution with attribution.

## Development process
Hikmalayer Core is developed in phases:

- **Phase 1**: Core PoW and chain primitives.
- **Phase 2**: PoS validator selection, staking, and validator identities.
- **Phase 3**: Persistence, P2P gossip, governance, slashing, and async‑safe services.
- **Phase 4**: Operational hardening, Dockerized multi-node deployment, monitoring, and benchmark validation. (Completed for API execution layer.)
- **Phase 5 (in progress)**: Public testnet with full P2P validator consensus and finalized-state tracking.


## Testing
Run the Rust test suite:

```bash
cargo test
```

## Automated testing
Automated tests are currently run via `cargo test` and include unit coverage for chain validation,
transactions, and PoS selection.

## Manual Quality Assurance testing
Manual QA can be performed using the API and dashboard:

- Start the backend (`cargo run`) and the dashboard (`npm run dev` in `dashboard/`).
- Verify mining, staking, transfers, and validation flows.
- Validate P2P peer registration and block gossip by running two nodes with different ports.

For secured environments, set `P2P_TOKEN` and `ADMIN_TOKEN` to require `x-p2p-token` and
`x-admin-token` headers for P2P and governance/slashing endpoints.

## Translations
No translations are included yet. If you want to add documentation translations, create locale‑
specific README files (for example `README.es.md`, `README.fr.md`).

## 📈 Performance Snapshot (Phase-4 Local Benchmark)

> Pre-mainnet API execution layer benchmark using Docker Compose multi-node deployment.

| Metric | Result |
|------|--------|
| Duration | 600 seconds |
| Total Transactions | 8,940 |
| Average Throughput | **14.88 TPS** |
| Average Latency | ~67 ms |
| Reorg Count | 0 (instrumentation pending) |
| Avg Memory per Node | ~4–5 MB |
| Deployment | Docker Compose (bootnode + validators + RPC) |

### Benchmark artifacts

```bash
bench/results/run_10min/
```

Includes:

- `benchmark_report.json`
- `benchmark_report.csv`
- `benchmark_report.md`

---

## 🏗 Phase-5 Roadmap (Public Testnet)

Phase-5 introduces peer-to-peer validator networking and public testnet deployment.

### Planned milestones

### Genesis & Network Bootstrap

- Deterministic genesis generation  
- Validator key provisioning  
- Initial stake distribution  

### Validator Roles

- Dedicated bootnode  
- Validator nodes  
- RPC / observer nodes  

### P2P Consensus Layer

- Validator gossip network  
- Block propagation  
- Fork handling  
- Finality depth tracking  

### Security Hardening

- Permissioned validator onboarding  
- Signed peer handshakes  
- Slashing enforcement  
- Replay protection  

### Public Testnet Deployment

- Multi-host deployment  
- External validators  
- Chain explorers  
- Public RPC endpoints  

---

## 📊 Architecture Overview

Current implementation provides:

- Hybrid PoS validator selection + PoW block finalization (logic implemented)  
- REST execution layer (benchmarked)  
- Governance + slashing primitives  
- Persistent chain state  
- Token subsystem  
- Smart contract execution framework  
- Dockerized orchestration  
- Monitoring + metrics  

### Upcoming (Phase-5)

- Validator networking  
- Block gossip  
- Finality tracking  
- Public testnet  

---

## 🚀 Ecosystem Note

Hikmalayer is designed as a trust-critical Layer-1 blockchain optimized for:

- Digital identity anchoring  
- Credential verification  
- Tokenized incentives  
- Validator accountability  

The architecture prioritizes:

- Deterministic validator selection  
- Cryptographic block finalization  
- Low operational overhead  
- Enterprise-grade deployability  

Phase-4 benchmarks demonstrate a stable execution foundation suitable for distributed network expansion.

---

## 🧭 Project Status

| Phase | Status |
|------|--------|
| Phase 1 | ✅ Complete |
| Phase 2 | ✅ Complete |
| Phase 3 | ✅ Complete |
| Phase 4 | ✅ Complete (Execution + Ops) |
| Phase 5 | 🚧 In Progress (Public Testnet) |



## Project directory
```
hikmalayer-core/
├── bench/
│   ├── benchmark.py
│   └── results/
│       ├── run_10min/
│       └── test_run/
├── dashboard/
│   ├── public/
│   ├── src/
│   │   ├── assets/
│   │   ├── components/
│   │   └── hooks/
│   ├── index.html
│   ├── package.json
│   └── vite.config.js
├── docs/
│   ├── API.md
│   ├── Whitepaper.md
│   ├── audit_readiness_pack.md
│   ├── benchmark_report.md
│   ├── consensus_flow.md
│   ├── key_management.md
│   ├── repo_readme_audit.md
│   ├── repository_code_audit.md
│   ├── security_hardening.md
│   ├── threat_model.md
│   ├── validator_lifecycle.md
│   └── whitepaper_short_version.md
├── ops/
│   ├── prometheus/
│   ├── README.md
│   ├── reset_chain.sh
│   ├── run_benchmark.sh
│   ├── start_testnet.sh
│   └── stop_testnet.sh
├── src/
│   ├── api/
│   ├── auth/
│   ├── blockchain/
│   ├── consensus/
│   ├── contract/
│   ├── p2p/
│   │   ├── mod.rs
│   │   ├── protocol.rs
│   │   └── service.rs
│   ├── token/
│   ├── governance.rs
│   ├── main.rs
│   └── persistence.rs
├── BENCHMARKING.md
├── CLA.md
├── Cargo.toml
├── Dockerfile
├── LICENSE
├── README.md
└── docker-compose.yml
```
