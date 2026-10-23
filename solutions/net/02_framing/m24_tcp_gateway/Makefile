.PHONY: build run test fmt lint check demo

SERVICE ?= api-gateway

build:
	cargo build --workspace

run:
	cargo run --quiet -p $(SERVICE)

test:
	cargo test --workspace

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt lint test

demo:
	cargo build --quiet -p api-gateway -p social-cli
	cargo run --quiet -p api-gateway & gateway=$$!; trap 'kill $$gateway' EXIT; cargo run --quiet -p social-cli
