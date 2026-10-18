#!/bin/sh
# Guided check for one P4-M5 scenario: after Dual Pane copies a file and another
# application replaces the clipboard, Paste must be refused. Opt-in: not part of
# `make check`.
#
# It REPLACES the macOS general clipboard and cannot reliably restore it. It
# runs a debug build against a disposable profile and works only inside
# temporary folders.
#
#   scripts/clipboard-replacement-check.sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=""
app_pid=""

cleanup() {
    if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
        kill "$app_pid" 2>/dev/null || true
    fi
    if [ -n "$work" ]; then
        rm -rf "$work"
    fi
}
trap cleanup EXIT

pause() {
    printf '%s\n  [Enter to continue] ' "$1"
    read -r _answer
}

clipboard_types() {
    osascript -e 'clipboard info' 2>/dev/null || true
}

has_file_url() {
    printf '%s' "$1" | grep -q furl
}

cat <<EOF
This guided check REPLACES your macOS clipboard and cannot reliably restore it.
It uses a debug build with a disposable profile and temporary folders only.
Close anything that depends on the current clipboard contents first.
EOF
printf 'Type "replace clipboard" to continue: '
read -r answer
if [ "$answer" != "replace clipboard" ]; then
    echo "Cancelled."
    exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-replacement-check.XXXXXX")
mkdir -p "$work/source" "$work/destination"
printf 'plain text\n' >"$work/source/plain.txt"
printf 'other\n' >"$work/source/other.txt"

cd "$repo_root"
cargo build -q -p dual-pane-desktop
# The work folder is the profile's Home, so a Home tab lists both folders.
DUAL_PANE_TEST_PROFILE="$work" "$repo_root/target/debug/dual-pane-desktop" "$work/source" &
app_pid=$!
sleep 2

echo
echo "  Left Browser source:   $work/source"
echo "  Right Browser target:  $work/destination"
pause "The Left Browser opened at source. In the Right Browser open destination from its Home tab ($work)."

pause "In the LEFT Browser select plain.txt only and press Command+C. Do NOT paste yet."
if [ -e "$work/destination/plain.txt" ]; then
    echo "  FAIL: plain.txt was pasted before the clipboard was replaced, so the check is invalid."
    exit 1
fi
before=$(clipboard_types)
echo "  Clipboard types after Dual Pane Copy: $before"
if ! has_file_url "$before"; then
    echo "  FAIL: Dual Pane Copy left no file URL on the clipboard, so the precondition is not met."
    exit 1
fi

printf 'replaced by another application' | pbcopy
after=$(clipboard_types)
echo "  Clipboard types after replacement:    $after"
if has_file_url "$after"; then
    echo "  FAIL: the script could not replace the clipboard; the precondition is not met."
    exit 1
fi

pause "Focus the RIGHT Browser and press Command+V exactly once. Paste must be refused with an explanation. Note the message shown."
echo "  Clipboard types at the end:           $(clipboard_types)"

if [ -e "$work/destination/plain.txt" ] || [ -L "$work/destination/plain.txt" ]; then
    echo "  FAIL: Replacement prevented any Paste"
    echo "  destination now holds: $(ls "$work/destination" | tr '\n' ' ')"
    exit 1
fi
if [ ! -f "$work/source/plain.txt" ]; then
    echo "  FAIL: the source file is missing"
    exit 1
fi
echo "  PASS: Replacement prevented any Paste"
