#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_home=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-path-columns.XXXXXX")
trap 'rm -rf "$test_home"' EXIT
test_home=$(CDPATH= cd -- "$test_home" && pwd -P)
mkdir -p "$test_home/Parent/Current" "$test_home/Parent/Sibling" "$test_home/Parent/bundle.app/Contents"
ln -s Current "$test_home/Parent/link"
cat > "$test_home/Parent/bundle.app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string></dict></plist>
PLIST

cd "$repo_root"
cargo build -q -p dual-pane-desktop
DUAL_PANE_TEST_PROFILE="$test_home" DUAL_PANE_INTERNAL_PATH_COLUMNS_ROOT="$test_home" QT_QPA_PLATFORM=offscreen "$repo_root/target/debug/dual-pane-desktop" --new-instance "$test_home/Parent/Current"
