.PHONY: build run install test fmt fmt-check lint-rust lint-cpp check

BIN_DIR ?= $(HOME)/.local/bin

build:
	cargo build

run:
	cargo run -p dual-pane-desktop

# Links the release binary as `dual-pane` so a rebuild updates the command.
install:
	cargo build --release -p dual-pane-desktop
	mkdir -p "$(BIN_DIR)"
	ln -sf "$(CURDIR)/target/release/dual-pane-desktop" "$(BIN_DIR)/dual-pane"
	@echo "Installed $(BIN_DIR)/dual-pane"

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

check:
	@set -e; \
	run_check() { \
		label="$$1"; \
		shift; \
		output="$$(mktemp)"; \
		printf '%s... ' "$$label"; \
		if "$$@" >"$$output" 2>&1; then \
			printf 'ok\n'; \
			rm -f "$$output"; \
		else \
			printf 'failed\n'; \
			rg -n -i 'error:|failed|panic|assertion' "$$output" | head -n 80 || sed -n '1,80p' "$$output"; \
			rm -f "$$output"; \
			exit 1; \
		fi; \
	}; \
	run_check 'Rust formatting' cargo fmt --all -- --check; \
	run_check 'C++ formatting' scripts/format-cpp.sh --check; \
	run_check 'Rust lint' cargo clippy --all-targets -- -D warnings; \
	run_check 'C++ lint' scripts/lint-cpp.sh; \
	run_check 'Tests' cargo test
