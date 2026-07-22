FROM rust:latest AS runner
WORKDIR /app

# Optional: speed + dependencies
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*

# Copy only manifest first (cache deps)
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true

# Now copy the real source
COPY . .

# Build real binary (optional; compose can run cargo run too)
RUN cargo build --release

EXPOSE 3000
CMD ["./target/release/hikmalayer"]
