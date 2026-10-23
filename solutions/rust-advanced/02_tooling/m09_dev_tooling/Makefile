.PHONY: build run test fmt lint check

SERVICE ?= repl

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
