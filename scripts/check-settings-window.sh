#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-settings-window.XXXXXX")
trap 'rm -rf "$test_home"' EXIT

cd "$repo_root"
cargo build -q -p dual-pane-desktop
HOME="$test_home" QT_QPA_PLATFORM=offscreen DUAL_PANE_INTERNAL_SETTINGS_WINDOW_CHECK=1 "$repo_root/target/debug/dual-pane-desktop"
