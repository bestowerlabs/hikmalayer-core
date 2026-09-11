# Automatic block production

Hikmalayer nodes do not mine automatically — `POST /mine` must be called
externally. `ops/devnet.sh` does this with a background shell loop; these
two files do the same thing under systemd, so it survives reboots, restarts
on failure, and logs errors instead of swallowing them.

## Install on any node (bootnode or validator)

    sudo cp hikmalayer-miner.sh /usr/local/bin/hikmalayer-miner.sh
    sudo chmod +x /usr/local/bin/hikmalayer-miner.sh
    sudo cp hikmalayer-miner.service /etc/systemd/system/hikmalayer-miner.service
    sudo systemctl daemon-reload
    sudo systemctl enable --now hikmalayer-miner.service

## Requirements

- `ADMIN_TOKEN` must be present in `~/hikmalayer-core/.env` on that node
- The node's API must be reachable at `127.0.0.1:3000`

## Verify it's working

    sudo systemctl status hikmalayer-miner.service
    curl http://127.0.0.1:3000/blockchain/stats   # run twice, a few seconds apart
    # total_blocks should increase with no manual /mine calls

## Block interval

Currently 5 seconds, matching `ops/devnet.sh`'s `BLOCK_SECONDS` default.
Change `BLOCK_SECONDS` in `hikmalayer-miner.sh` if this should differ on
mainnet.
