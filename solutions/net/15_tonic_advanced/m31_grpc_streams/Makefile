.PHONY: build run run-all test fmt lint check demo

SERVICE ?= api-gateway

SERVICES = user-service post-service chat-service auth-service api-gateway

build:
	cargo build --workspace

run:
	cargo run --quiet -p $(SERVICE)

run-all:
	cargo build --quiet --workspace --bins
	trap 'kill 0' INT TERM; for service in $(SERVICES); do cargo run --quiet -p $$service & done; wait

test:
	cargo test --workspace

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt lint test

demo:
	cargo build --quiet --workspace --bins
	pids=""; for service in $(SERVICES); do cargo run --quiet -p $$service & pids="$$pids $$!"; done; trap 'kill -INT $$pids' EXIT; cargo run --quiet -p social-cli
