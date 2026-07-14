# Local Docker Testnet Deployment Result

Date: 14 July 2026
Environment: Windows 11 + WSL2 (Ubuntu 26.04) + Docker Desktop (WSL2 backend)

## Containers Running
```
NAME                       SERVICE         STATUS   PORTS
hikmalayer-bootnode        bootnode        Up       0.0.0.0:3000->3000/tcp
hikmalayer-validator1      validator1      Up       0.0.0.0:3001->3000/tcp
hikmalayer-validator2      validator2      Up       0.0.0.0:3002->3000/tcp
hikmalayer-validator3      validator3      Up       0.0.0.0:3003->3000/tcp
hikmalayer-validator4      validator4      Up       0.0.0.0:3004->3000/tcp
hikmalayer-rpc             rpc             Up       0.0.0.0:3010->3000/tcp
hikmalayer-json-exporter   json_exporter   Up       0.0.0.0:7979->7979/tcp
hikmalayer-prometheus      prometheus      Up       0.0.0.0:9090->9090/tcp
hikmalayer-grafana         grafana         Up       0.0.0.0:3005->3000/tcp
```
All 9/9 containers reached a healthy `Up` state via `./ops/start_testnet.sh`.

## On-Chain Validator Set
```json
[
  {"address":"hkm016bfe32723b4ceab2584aecceca763afc2c2e69","stake":100,"public_key":"04f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9388f7b0f632de8140fe337e62a37f3566500a99934c2231b6cb9fd7584b8e672"},
  {"address":"hkm50929b74c1a04954b78b4b6035e97a5e078a5a0f","stake":1000,"public_key":"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8"},
  {"address":"hkm663c1b408cf896dd00e20c84b2e734b5d8da0893","stake":100,"public_key":"04c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee51ae168fea63dc339a3c58419466ceaeef7f632653266d0e1236431a950cfe52a"}
]
```

## Blockchain State
```json
{"height":4,"state_root":"95e26ee5ca32eb7388960250c7a9826ef0be9863ccab5d6c0ea3d19001c40890","total_supply":1000020,"burned":0,"staked":1200,"validators":3,"accounts":4}
```

## Notes
- Deployed via `./ops/start_testnet.sh` with default `P2P_TOKEN` / `ADMIN_TOKEN` (`local-testnet` / `local-admin`).
- The stock `Dockerfile` CMD (`cargo run --release`) failed inside containers with `error: 'cargo run' could not determine which binary to run` once the `hikma-wallet` binary was added alongside the main `hikmalayer` binary. Fixed by pinning the entrypoint: `CMD ["cargo", "run", "--release", "--bin", "hikmalayer"]`. This should be committed so future clean builds don't hit the same failure.
- 3 validators successfully registered on-chain via signed staking transactions (bootnode + validator1 + validator2 funding/staking flow).
- Chain height 4, valid state root, total supply and staked balances consistent with on-chain execution.
- Confirms end-to-end local deployment: build, container orchestration, faucet funding, on-chain staking, mining, and state root computation all functioning correctly.
- Deployment was performed on an 8GB RAM Windows machine; WSL2 memory was capped via `.wslconfig` (`memory=5GB`) to leave headroom for the host OS.
