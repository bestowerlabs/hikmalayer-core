# Hikmalayer Deployment Guide (Beginner → Professional)

> **Goal:** Deploy Hikmalayer Core safely in a production-style environment with API, validators, monitoring, and dashboard explorer.

---

## 1) What you are deploying

A complete Hikmalayer stack includes:

- **Core blockchain nodes** (bootnode + validators + optional RPC/observer)
- **REST API** (served by each node process)
- **Dashboard explorer frontend** (`dashboard/`)
- **Monitoring stack** (Prometheus + Grafana)
- **Operational scripts** (`ops/`)

For local orchestration, this repository already provides `docker-compose.yml` and Prometheus configs under `ops/prometheus/`.

---

## 2) Minimum production-ready prerequisites

## Infrastructure

- Linux host(s), preferably Ubuntu 22.04+ (single host for staging, multi-host for production)
- 2+ vCPU, 4+ GB RAM per node (increase for real workloads)
- Public/static IP or internal load balancer for RPC/API nodes
- Persistent disk volume for node state backups

## Software

- Docker + Docker Compose (for containerized deployment)
- Rust toolchain (if running directly without Docker)
- Node.js 20+ and npm (for dashboard build)

## Security basics (must do)

- Set strong `P2P_TOKEN` and `ADMIN_TOKEN`
- Restrict public exposure of admin/governance endpoints
- Enable HTTPS/TLS at reverse proxy layer (Nginx/Traefik/Caddy)
- Use firewall rules to allow only required ports
- Rotate secrets periodically

---

## 3) Repository setup

```bash
git clone <your-fork-or-origin-url>
cd hikmalayer-core
```

Create environment values (do **not** commit secrets):

```bash
export P2P_TOKEN="replace-with-strong-random-token"
export ADMIN_TOKEN="replace-with-strong-random-token"
export RUST_LOG="info"
```

---

## 4) Local/staging deployment (recommended first)

Use the included Docker Compose stack:

```bash
docker compose up -d --build
```

This starts:

- bootnode + validator nodes + rpc
- json exporter + prometheus + grafana

Check health:

```bash
docker compose ps
curl -s http://127.0.0.1:3000/blockchain/stats
curl -s http://127.0.0.1:3000/explorer/overview
```

Logs:

```bash
docker compose logs -f bootnode
```

Stop stack:

```bash
docker compose down
```

---

## 5) Dashboard + Explorer deployment

The explorer UI is part of the dashboard app and consumes backend endpoints such as:

- `/explorer/overview`
- `/explorer/blocks`
- `/explorer/blocks/index/{index}`
- `/explorer/blocks/hash/{hash}`
- `/explorer/search/{query}`
- `/explorer/transactions/pending`

Build dashboard for production:

```bash
cd dashboard
npm ci
npm run build
```

Serve `dashboard/dist/` via Nginx/Caddy and route API calls to your RPC/API node.

### Example reverse-proxy pattern

- `https://explorer.example.com` → static dashboard files
- `https://api.example.com` → Hikmalayer API node(s)

If you use a single domain, configure reverse proxy path routing:

- `/` → dashboard static files
- `/api/*` → upstream Hikmalayer node, rewrite to backend routes

---

## 6) Professional production topology

Use this pattern for reliability:

1. **Bootnode tier** (private networking)
2. **Validator tier** (not publicly exposed)
3. **RPC tier** (public/read-heavy clients)
4. **Explorer frontend tier** (static CDN or reverse-proxied)
5. **Observability tier** (Prometheus + Grafana + alerts)

Recommended:

- At least 3–5 validators
- Dedicated RPC nodes for user traffic
- Separate monitoring host or namespace
- Daily state snapshots + offsite backups

---

## 7) Security hardening checklist (critical)

- [ ] P2P/admin tokens are set and not default
- [ ] Governance/slashing endpoints restricted by token + network ACL
- [ ] TLS enabled end-to-end or at ingress
- [ ] CORS configured only for trusted explorer origin(s)
- [ ] Container images pinned and regularly updated
- [ ] Secrets stored in secret manager (not `.env` in repo)
- [ ] Automated backups tested for restore
- [ ] Alerting configured (node down, high latency, API errors)
- [ ] Access logs and audit logs retained

---

## 8) Validation before go-live (do not skip)

Run these checks:

```bash
# Backend tests
cargo test

# Dashboard production build
cd dashboard && npm run build

# API sanity
curl -s http://127.0.0.1:3000/blockchain/stats
curl -s http://127.0.0.1:3000/explorer/overview
curl -s "http://127.0.0.1:3000/explorer/blocks?offset=0&limit=10"
```

Operational verification:

- Issue certificate / transfer token
- Mine block
- Confirm explorer lists latest block and pending tx behavior
- Confirm monitoring dashboards and targets are healthy

---

## 9) Zero-downtime update strategy

1. Build/test new release in staging first
2. Backup chain state and configs
3. Roll out to RPC nodes first (canary)
4. Roll out validators one-by-one
5. Verify chain health and explorer consistency after each step
6. Keep rollback artifact ready (previous image/tag)

---

## 10) Common mistakes to avoid

- Exposing admin endpoints publicly without token/network restriction
- Running validators and public RPC on same weak host
- Skipping backups/restoration testing
- Deploying dashboard without confirming API endpoint compatibility
- Ignoring TLS and secret management

---

## 11) Quick command reference

```bash
# Start packaged testnet stack
./ops/start_testnet.sh

# Stop packaged testnet stack
./ops/stop_testnet.sh

# Reset local chain state (careful)
./ops/reset_chain.sh

# Run benchmark harness
./ops/run_benchmark.sh
```

---

## Final note

For beginners: always deploy in **staging first**, verify explorer + API + monitoring together, then promote to production.

For professional teams: enforce CI checks, immutable image tags, staged rollouts, and incident response runbooks.
