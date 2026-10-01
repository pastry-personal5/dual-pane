#!/bin/sh

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
check_mode=false

if [ "${1:-}" = "--check" ]; then
    check_mode=true
fi

if [ -n "${CLANG_FORMAT:-}" ]; then
    clang_format=$CLANG_FORMAT
elif command -v clang-format >/dev/null 2>&1; then
    clang_format=$(command -v clang-format)
else
    clang_format="$(brew --prefix llvm)/bin/clang-format"
fi

if [ ! -x "$clang_format" ]; then
    echo "clang-format was not found; install LLVM or set CLANG_FORMAT" >&2
    exit 1
fi

set -- "$repository_root/crates/dual-pane-desktop/cpp/include/dual_pane_desktop/desktop_window.hpp" "$repository_root/crates/dual-pane-desktop/cpp/include/dual_pane_desktop/settings_glyph.hpp" "$repository_root/crates/dual-pane-desktop/cpp/src/desktop_window.cpp"

if [ "$check_mode" = true ]; then
    exec "$clang_format" --dry-run --Werror "$@"
fi

exec "$clang_format" -i "$@"
