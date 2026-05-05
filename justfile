default:
    @just --list

run-native:
    cargo run -p client --release

# Two-peer P2P; run `just serve-signaling` first, then this in two terminals.
run-net:
    cargo run -p client --release -- --net

# Offline rollback validator: panics on any non-determinism in the sim.
sync-test:
    cargo run -p client --release -- --sync-test

# Local matchbox signaling server on :3536 (auto-installs on first run).
serve-signaling:
    @command -v matchbox_server >/dev/null 2>&1 || cargo install --locked matchbox_server
    matchbox_server 0.0.0.0:3536

watch:
    cargo watch -x "run -p client --release -- --replay"

build-wasm:
    cargo build -p client --release --target wasm32-unknown-unknown
    @cp target/wasm32-unknown-unknown/release/head-on-client.wasm web/head-on-client.wasm
    @echo 'wasm copied to web/head-on-client.wasm — run "just serve-wasm" to play'

serve-wasm: build-wasm
    @echo "serving on http://localhost:8080"
    @cd web && python3 -m http.server 8080

test:
    cargo test -p sim

fmt:
    cargo fmt --all

clippy:
    cargo clippy --all-targets -- -D warnings
