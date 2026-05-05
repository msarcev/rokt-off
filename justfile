default:
    @just --list

run-native:
    cargo run -p client --release

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
