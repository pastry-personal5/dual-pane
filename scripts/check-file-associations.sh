#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_profile=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-file-associations.XXXXXX")
trap 'rm -rf "$test_profile"' EXIT
test_profile=$(CDPATH= cd -- "$test_profile" && pwd -P)
mkdir "$test_profile/Files"
printf '{}\n' > "$test_profile/Files/first.json"
printf '{}\n' > "$test_profile/Files/second.json"

cd "$repo_root"
cargo test -q -p dual-pane-desktop native_shell::tests::file_association_discovery_uses_a_temporary_regular_file
cargo build -q -p dual-pane-desktop
DUAL_PANE_TEST_PROFILE="$test_profile" DUAL_PANE_INTERNAL_FILE_ASSOCIATIONS_ROOT="$test_profile/Files" QT_QPA_PLATFORM=offscreen "$repo_root/target/debug/dual-pane-desktop" --new-instance "$test_profile/Files"
