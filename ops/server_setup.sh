#!/usr/bin/env bash
# Hikmalayer Cherry Servers Node Setup Script
# Run on each fresh Ubuntu 22.04 instance after SSH in
set -euo pipefail

echo "=== Step 1: System update ==="
sudo apt update && sudo apt upgrade -y

echo "=== Step 2: Install dependencies ==="
sudo apt install -y ufw fail2ban ca-certificates curl gnupg git \
  build-essential pkg-config libssl-dev

echo "=== Step 3: Configure firewall ==="
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 22/tcp
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw --force enable
sudo ufw status

echo "=== Step 4: Enable fail2ban ==="
sudo systemctl enable --now fail2ban

echo "=== Step 5: Install Docker ==="
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER

echo "=== Step 6: Configure Docker log rotation ==="
sudo tee /etc/docker/daemon.json << 'DAEMON_EOF'
{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3"
  }
}
DAEMON_EOF
sudo systemctl restart docker
newgrp docker

echo "=== Step 7: Install Rust ==="
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

echo "=== Step 8: Clone repository ==="
git clone https://github.com/bestowerlabs/hikmalayer-core.git
cd hikmalayer-core

echo "=== Step 9: Build binaries ==="
cargo build --release

echo "=== Step 10: Generate validator key ==="
echo ""
echo "Your validator key pair:"
./target/release/hikma-wallet keygen
echo ""
echo "=== IMPORTANT ==="
echo "1. Save the private_key securely — add to .env as VALIDATOR_PRIVATE_KEY"
echo "2. Send the public_key and address to your supervisor"
echo "3. NEVER share your private_key with anyone"
echo "4. Request P2P_TOKEN and ADMIN_TOKEN from supervisor"
echo ""
echo "=== Setup complete ==="
echo "Next: Create .env from validator.env.example and start containers"
