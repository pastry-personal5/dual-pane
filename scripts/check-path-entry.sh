#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-path-entry.XXXXXX")
trap 'rm -rf "$test_home"' EXIT
test_home=$(CDPATH= cd -- "$test_home" && pwd -P)
mkdir "$test_home/Start" "$test_home/Stored"
: > "$test_home/Start/keep"

cd "$repo_root"
cargo build -q -p dual-pane-desktop
DUAL_PANE_TEST_PROFILE="$test_home" DUAL_PANE_INTERNAL_PATH_ENTRY_ROOT="$test_home" QT_QPA_PLATFORM=offscreen "$repo_root/target/debug/dual-pane-desktop" --new-instance "$test_home/Start"
