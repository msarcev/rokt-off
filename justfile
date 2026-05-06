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
    @command -v wasm-bindgen >/dev/null 2>&1 || cargo install --locked wasm-bindgen-cli@0.2.120
    cargo build -p client --release --target wasm32-unknown-unknown
    rm -rf web/pkg
    wasm-bindgen --target no-modules --no-typescript --out-dir web/pkg \
        target/wasm32-unknown-unknown/release/rokt_off_client.wasm
    # Expose wasm-bindgen's instance to gl.js globals so miniquad's import
    # bridges can read Rust memory. Patch right after `wasm = instance.exports`.
    sed -i.bak 's/wasm = instance.exports;/wasm = instance.exports; globalThis.wasm_memory = wasm.memory; globalThis.wasm_exports = wasm;/' web/pkg/rokt_off_client.js
    rm -f web/pkg/rokt_off_client.js.bak
    @echo 'wasm in web/pkg/rokt_off_client_bg.wasm — run "just serve-wasm" to play'

serve-wasm: build-wasm
    @echo "serving on http://localhost:8080"
    @cd web && python3 -m http.server 8080

test:
    cargo test -p sim

fmt:
    cargo fmt --all

clippy:
    cargo clippy --all-targets -- -D warnings
