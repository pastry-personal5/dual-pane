.PHONY: build run install package-macos install-macos test fmt fmt-check lint-rust lint-cpp check

BIN_DIR ?= $(HOME)/.local/bin
APP_BUNDLE ?= $(CURDIR)/target/package/Dual Pane.app
APP_INSTALL_DIR ?= /Applications
INSTALLED_APP ?= $(APP_INSTALL_DIR)/Dual Pane.app

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

# Creates an unnotarized macOS application bundle for local use.
package-macos:
	cargo build --release -p dual-pane-desktop
	rm -rf "$(APP_BUNDLE)"
	mkdir -p "$(APP_BUNDLE)/Contents/MacOS" "$(APP_BUNDLE)/Contents/Resources"
	cp target/release/dual-pane-desktop "$(APP_BUNDLE)/Contents/MacOS/dual-pane"
	cp crates/dual-pane-desktop/cpp/resources/dual-pane-icon.png "$(APP_BUNDLE)/Contents/Resources/dual-pane-icon.png"
	cp packaging/macos/Info.plist "$(APP_BUNDLE)/Contents/Info.plist"
	@echo "Created $(APP_BUNDLE)"

# Installs the application for the current user and exposes its executable on PATH.
install-macos: package-macos
	rm -rf "$(INSTALLED_APP)"
	mkdir -p "$(APP_INSTALL_DIR)"
	cp -R "$(APP_BUNDLE)" "$(INSTALLED_APP)"
	mkdir -p "$(BIN_DIR)"
	ln -sfn "$(INSTALLED_APP)/Contents/MacOS/dual-pane" "$(BIN_DIR)/dual-pane"
	@echo "Installed $(INSTALLED_APP)"
	@echo "Command: $(BIN_DIR)/dual-pane"

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
