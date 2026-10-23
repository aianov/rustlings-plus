.PHONY: build run run-all test fmt lint check demo certs

SERVICE ?= api-gateway

SERVICES = user-service post-service chat-service auth-service api-gateway

build:
	cargo build --workspace

run:
	cargo run --quiet -p $(SERVICE)

certs:
	cargo xtask certs --keep

run-all: certs
	cargo build --quiet --workspace --bins
	trap 'kill 0' INT TERM; for service in $(SERVICES); do cargo run --quiet -p $$service & done; wait

test:
	cargo test --workspace

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt lint test

demo: certs
	cargo build --quiet --workspace --bins
	pids=""; for service in $(SERVICES); do cargo run --quiet -p $$service & pids="$$pids $$!"; done; trap 'kill -TERM $$pids 2>/dev/null; wait' EXIT; cargo run --quiet -p social-cli
