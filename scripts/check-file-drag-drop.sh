#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-file-drag-drop.XXXXXX")
trap 'rm -rf "$test_home"' EXIT
test_home=$(CDPATH= cd -- "$test_home" && pwd -P)
mkdir "$test_home/Source" "$test_home/Destination" "$test_home/Destination/Folder"
: > "$test_home/Source/copy-me"
: > "$test_home/Source/move-me"
: > "$test_home/Source/row-target"

cd "$repo_root"
cargo test -q -p dual-pane-application --test drag_drop
cargo build -q -p dual-pane-desktop
DUAL_PANE_TEST_PROFILE="$test_home" DUAL_PANE_INTERNAL_FILE_DRAG_DROP_ROOT="$test_home" QT_QPA_PLATFORM=offscreen "$repo_root/target/debug/dual-pane-desktop" --new-instance "$test_home/Source"
