#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-folder-items-context-menu.XXXXXX")
trap 'rm -rf "$test_home"' EXIT

cd "$repo_root"
cargo test -q -p dual-pane-application --test context_menus
cargo build -q -p dual-pane-desktop
DUAL_PANE_TEST_PROFILE="$test_home" QT_QPA_PLATFORM=offscreen DUAL_PANE_INTERNAL_FOLDER_ITEMS_CONTEXT_MENU_CHECK=1 "$repo_root/target/debug/dual-pane-desktop" --new-instance
