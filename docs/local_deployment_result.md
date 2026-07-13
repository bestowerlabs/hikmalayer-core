# Local Docker Testnet Deployment Result

Date: 2026-07-14

## Containers Running
```
NAME                       IMAGE                                      COMMAND                  SERVICE         CREATED          STATUS          PORTS
hikmalayer-bootnode        hikmalayer-core-bootnode                   "cargo run --release…"   bootnode        13 minutes ago   Up 13 minutes   0.0.0.0:3000->3000/tcp, [::]:3000->3000/tcp
hikmalayer-grafana         grafana/grafana:latest                     "/run.sh"                grafana         13 minutes ago   Up 13 minutes   0.0.0.0:3005->3000/tcp, [::]:3005->3000/tcp
hikmalayer-json-exporter   prometheuscommunity/json-exporter:latest   "/bin/json_exporter …"   json_exporter   13 minutes ago   Up 13 minutes   0.0.0.0:7979->7979/tcp, [::]:7979->7979/tcp
hikmalayer-prometheus      prom/prometheus:latest                     "/bin/prometheus --c…"   prometheus      13 minutes ago   Up 13 minutes   0.0.0.0:9090->9090/tcp, [::]:9090->9090/tcp
hikmalayer-rpc             hikmalayer-core-rpc                        "cargo run --release…"   rpc             13 minutes ago   Up 13 minutes   0.0.0.0:3010->3000/tcp, [::]:3010->3000/tcp
hikmalayer-validator1      hikmalayer-core-validator1                 "cargo run --release…"   validator1      13 minutes ago   Up 2 minutes    0.0.0.0:3001->3000/tcp, [::]:3001->3000/tcp
hikmalayer-validator2      hikmalayer-core-validator2                 "cargo run --release…"   validator2      13 minutes ago   Up 2 minutes    0.0.0.0:3002->3000/tcp, [::]:3002->3000/tcp
hikmalayer-validator3      hikmalayer-core-validator3                 "cargo run --release…"   validator3      13 minutes ago   Up 2 minutes    0.0.0.0:3003->3000/tcp, [::]:3003->3000/tcp
hikmalayer-validator4      hikmalayer-core-validator4                 "cargo run --release…"   validator4      13 minutes ago   Up 2 minutes    0.0.0.0:3004->3000/tcp, [::]:3004->3000/tcp
```

## On-Chain Validator Set
```
[{"address":"hkm016bfe32723b4ceab2584aecceca763afc2c2e69","stake":100,"public_key":"04f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9388f7b0f632de8140fe337e62a37f3566500a99934c2231b6cb9fd7584b8e672"},{"address":"hkm50929b74c1a04954b78b4b6035e97a5e078a5a0f","stake":1000,"public_key":"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8"},{"address":"hkm663c1b408cf896dd00e20c84b2e734b5d8da0893","stake":100,"public_key":"04c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee51ae168fea63dc339a3c58419466ceaeef7f632653266d0e1236431a950cfe52a"}]```

## Blockchain State
```
{"height":4,"state_root":"95e26ee5ca32eb7388960250c7a9826ef0be9863ccab5d6c0ea3d19001c40890","total_supply":1000020,"burned":0,"staked":1200,"validators":3,"accounts":4}```

## Notes
- Deployed via ./ops/start_testnet.sh with default P2P_TOKEN/ADMIN_TOKEN (local-testnet / local-admin).
- All 9 containers healthy: bootnode, validator1-4, rpc, prometheus, grafana, json-exporter.
- 3 validators successfully registered on-chain via signed staking transactions (bootnode + validator1 + validator2).
- Chain height 4, valid state root, total supply and staked balances consistent with on-chain execution.
- Confirms end-to-end local deployment: build, container orchestration, faucet funding, on-chain staking, mining, and state root computation all functioning correctly.
