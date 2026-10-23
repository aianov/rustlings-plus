SERVICE ?= api
PROFILE ?= debug
CARGO_FLAGS = $(if $(filter release,$(PROFILE)),--release,)

.PHONY: build run

build:
	cargo build $(CARGO_FLAGS)

run:
	cargo run --quiet $(CARGO_FLAGS) -p $(SERVICE)
