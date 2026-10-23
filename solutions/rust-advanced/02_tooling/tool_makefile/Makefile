.PHONY: all build test fmt lint check

all: check

build:
	cargo build

test: build
	cargo test

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --all-targets -- -D warnings

check: fmt lint test
