#!/usr/bin/env python3
from http.server import HTTPServer, BaseHTTPRequestHandler
import urllib.request
import json

NODES = {
    "bootnode":   "http://bootnode:3000",
    "validator1": "http://validator1:3000",
    "validator2": "http://validator2:3000",
    "validator3": "http://validator3:3000",
    "validator4": "http://validator4:3000",
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
