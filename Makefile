.PHONY: build run test fmt fmt-check lint-rust lint-cpp check

build:
	cargo build

run:
	cargo run -p dual-pane-desktop

test:
	cargo test

fmt:
	cargo fmt --all
	scripts/format-cpp.sh

fmt-check:
	cargo fmt --all -- --check
	scripts/format-cpp.sh --check

lint-rust:
	cargo clippy --all-targets -- -D warnings

lint-cpp:
	scripts/lint-cpp.sh

check: fmt-check lint-rust lint-cpp test
