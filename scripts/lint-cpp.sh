#!/bin/sh

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ -n "${CLANG_TIDY:-}" ]; then
    clang_tidy=$CLANG_TIDY
elif command -v clang-tidy >/dev/null 2>&1; then
    clang_tidy=$(command -v clang-tidy)
else
    clang_tidy="$(brew --prefix llvm)/bin/clang-tidy"
fi

if [ ! -x "$clang_tidy" ]; then
    echo "clang-tidy was not found; install LLVM or set CLANG_TIDY" >&2
    exit 1
fi

if [ -n "${QMAKE:-}" ]; then
    qmake=$QMAKE
elif command -v qmake >/dev/null 2>&1; then
    qmake=$(command -v qmake)
elif command -v qmake6 >/dev/null 2>&1; then
    qmake=$(command -v qmake6)
else
    echo "qmake was not found; set QMAKE to the Qt 6 qmake executable" >&2
    exit 1
fi

qt_version=$("$qmake" -query QT_VERSION)
if ! awk -v version="$qt_version" 'BEGIN { split(version, parts, "."); exit !(parts[1] > 6 || (parts[1] == 6 && (parts[2] > 11 || (parts[2] == 11 && parts[3] >= 2)))) }'; then
    echo "Qt $qt_version is below the required 6.11.2" >&2
    exit 1
fi

qt_headers=$("$qmake" -query QT_INSTALL_HEADERS)
qt_libraries=$("$qmake" -query QT_INSTALL_LIBS)
sdk_path=$(xcrun --show-sdk-path)

cd "$repository_root"

for source in crates/dual-pane-desktop/cpp/include/dual_pane_desktop/desktop_window.hpp crates/dual-pane-desktop/cpp/src/desktop_window.cpp; do
    "$clang_tidy" --config-file="$repository_root/.clang-tidy" --warnings-as-errors='*' "$source" -- -std=c++17 -isysroot "$sdk_path" -I"$repository_root/crates/dual-pane-desktop/cpp/include" -isystem "$qt_headers" -F "$qt_libraries" -iframework "$qt_libraries"
done
