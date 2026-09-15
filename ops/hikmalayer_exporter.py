#!/usr/bin/env python3
from http.server import HTTPServer, BaseHTTPRequestHandler
import urllib.request
import json

NODES = {
    "bootnode":   "http://bootnode:3000",
    "validator1": "http://84.32.108.199:3001",
    "validator2": "http://88.216.210.55:3001",
}

COUNTER_METRICS = {
    "blocks_mined":               "Total blocks mined by this node",
    "blocks_received":            "Total blocks received via P2P",
    "blocks_rejected":            "Total blocks rejected",
    "reorgs":                     "Total chain reorganisations",
    "transactions_received":      "Total transactions received",
    "peers_registered":           "Total peers registered since startup",
    "peers_banned":               "Total peers banned",
    "slashes_submitted":          "Total slashing events",
    "gossip_sent":                "Total gossip messages sent",
    "gossip_failed":              "Total gossip messages failed",
    "protocol_messages_received": "Total P2P messages received",
    "protocol_messages_rejected": "Total P2P messages rejected",
    "invalid_from_peers":         "Total invalid messages from peers",
}
# Remembers each token's total_supply from the previous poll, so we can
# detect a drop (a burn) between one scrape and the next. This lives in
# the exporter's own process memory — it resets if the exporter restarts,
# which only means one missed comparison, not a wrong one.
_last_seen_supply = {}

class MetricsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        output = []
        for node, base_url in NODES.items():
            try:
                # Scrape counter metrics
                with urllib.request.urlopen(f"{base_url}/metrics", timeout=3) as r:
                    data = json.loads(r.read())
                for key, help_text in COUNTER_METRICS.items():
                    if key in data:
                        metric_name = f"hikmalayer_{key}"
                        output.append(f"# HELP {metric_name} {help_text}")
                        output.append(f"# TYPE {metric_name} gauge")
                        output.append(f'hikmalayer_{key}{{node="{node}"}} {data[key]}')
            except Exception as e:
                output.append(f"# ERROR scraping {node} metrics: {e}")

            try:
                # Scrape live peer count from explorer
                with urllib.request.urlopen(f"{base_url}/explorer/overview", timeout=3) as r:
                    overview = json.loads(r.read())
                peer_count = overview.get("peers", 0)
                output.append(f"# HELP hikmalayer_peer_count Current number of connected peers")
                output.append(f"# TYPE hikmalayer_peer_count gauge")
                output.append(f'hikmalayer_peer_count{{node="{node}"}} {peer_count}')

                # Also scrape finalized height and total blocks
                total_blocks = overview.get("total_blocks", 0)
                finalized = overview.get("finalized_height", 0)
                chain_valid = 1 if overview.get("chain_valid", False) else 0
                output.append(f"# HELP hikmalayer_total_blocks Total blocks in chain")
                output.append(f"# TYPE hikmalayer_total_blocks gauge")
                output.append(f'hikmalayer_total_blocks{{node="{node}"}} {total_blocks}')
                output.append(f"# HELP hikmalayer_finalized_height Latest finalized block height")
                output.append(f"# TYPE hikmalayer_finalized_height gauge")
                output.append(f'hikmalayer_finalized_height{{node="{node}"}} {finalized}')
                output.append(f"# HELP hikmalayer_chain_valid Whether the chain is valid")
                output.append(f"# TYPE hikmalayer_chain_valid gauge")
                output.append(f'hikmalayer_chain_valid{{node="{node}"}} {chain_valid}')
            except Exception as e:
                output.append(f"# ERROR scraping {node} overview: {e}")
                 # Burn detection: compare each token's total_supply against what we
        # saw last poll. A drop means units were burned since then. This is
        # deliberately external to the chain's own consensus code — it
        # observes public API output only, so it can never affect what the
        # chain considers valid.
        try:
            with urllib.request.urlopen(f"{list(NODES.values())[0]}/assets", timeout=3) as r:
                assets = json.loads(r.read())
            total_burned_since_start = 0
            for asset in assets:
                token_id = asset.get("token_id")
                supply = asset.get("total_supply", 0)
                previous = _last_seen_supply.get(token_id)
                if previous is not None and supply < previous:
                    burned_this_poll = previous - supply
                    output.append(f"# HELP hikmalayer_token_burn_detected A burn was detected this poll for this token")
                    output.append(f"# TYPE hikmalayer_token_burn_detected gauge")
                    output.append(f'hikmalayer_token_burn_detected{{token_id="{token_id}"}} {burned_this_poll}')
                _last_seen_supply[token_id] = supply
        except Exception as e:
            output.append(f"# ERROR scraping /assets for burn detection: {e}")   
        body = "\n".join(output) + "\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; version=0.0.4")
        self.end_headers()
        self.wfile.write(body.encode())
    
    def log_message(self, format, *args):
        pass

if __name__ == "__main__":
    print("Hikmalayer Prometheus exporter running on :8000")
    HTTPServer(("0.0.0.0", 8000), MetricsHandler).serve_forever()
  
