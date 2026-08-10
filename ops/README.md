# Ops & Deployment

Scripts for running Hikmalayer locally and in multi-node deployments.

## Scripts

| Script | What it does |
|---|---|
| `devnet.sh` | **Start here.** A complete local chain in one command: funded treasury, registered validator, and a miner sealing a block every few seconds so queued transactions actually execute |
| `start_testnet.sh` / `stop_testnet.sh` | A local multi-node testnet |
| `reset_chain.sh` | Clear local chain state |
| `run_benchmark.sh` | Drive the benchmark harness in `bench/` |
| `prometheus/` | Monitoring configuration for the Compose stack |

## One-command devnet

```bash
ops/devnet.sh
```

Prints the treasury address, admin token and RPC URL, then seals blocks
continuously. Useful environment overrides:

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | 3000 | RPC port |
| `CHAIN_ID` | `hikmalayer-devnet` | Network id — part of every signature |
| `BLOCK_SECONDS` | 5 | Sealing interval |
| `ADMIN_TOKEN` | `devadmin` | Admin endpoints |
| `KEEP_STATE` | 0 | `1` keeps `.devnet/` across restarts |
| `DEVNET_KEY` | fixed dev key | Override for a fresh chain |

**Development only.** It uses fixed keys, prints secrets, and enables the
faucet. Nothing here belongs on a network you care about.

## Multi-node with monitoring

```bash
docker compose up -d --build
```

Starts a bootnode, validator nodes, an RPC node, a JSON exporter, Prometheus
and Grafana. See `BENCHMARKING.md` for the benchmark harness.

## Production deployment

`ops/` is for development and testing. For a real network — genesis parameters,
key management, TLS, rate limiting and monitoring — see:

- [`docs/deployment_guide.md`](../docs/deployment_guide.md)
- [`docs/key_management.md`](../docs/key_management.md)
- [`docs/validator_lifecycle.md`](../docs/validator_lifecycle.md)
- [`docs/mainnet_readiness.md`](../docs/mainnet_readiness.md)

**Genesis parameters are chain identity.** Chain id, supply, validator
allowlist, hybrid requirement and the genesis validator's keys are all committed
to by the genesis state root. Two nodes that disagree on any of them are running
different networks.
