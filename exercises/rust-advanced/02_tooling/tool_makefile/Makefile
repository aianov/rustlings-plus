# Makefile для greeter. make build пишет про отсутствующий разделитель, а make test
# уверяет, что делать нечего. Цели: build, test (после build), fmt, lint, check (fmt, lint и test).
# Цель по умолчанию это check.
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
