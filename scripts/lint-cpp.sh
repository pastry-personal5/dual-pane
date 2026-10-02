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

# The shim includes headers that CXX-Qt generates while building the desktop crate. Cargo reports
# that crate's build-script output directory, which holds them under `cxxqtbuild/include`.
desktop_out_dir=$(cargo build -q -p dual-pane-desktop --message-format=json | sed -n 's/^{"reason":"build-script-executed","package_id":"[^"]*dual-pane-desktop#[^"]*".*"out_dir":"\([^"]*\)"}$/\1/p' | tail -n 1)
generated_headers="$desktop_out_dir/cxxqtbuild/include"
if [ -z "$desktop_out_dir" ] || [ ! -d "$generated_headers" ]; then
    echo "CXX-Qt generated headers were not found; check that 'cargo build -p dual-pane-desktop' succeeds" >&2
    exit 1
fi

# Some CXX-Qt library headers include Qt Core headers without the `QtCore/` prefix, as the
# compiler invocation in the build allows; give clang-tidy the same Qt Core header directory.
qt_core_headers="$qt_libraries/QtCore.framework/Headers"

for source in crates/dual-pane-desktop/cpp/include/dual_pane_desktop/desktop_window.hpp crates/dual-pane-desktop/cpp/include/dual_pane_desktop/settings_glyph.hpp crates/dual-pane-desktop/cpp/include/dual_pane_desktop/native_shell.hpp crates/dual-pane-desktop/cpp/src/desktop_window.cpp crates/dual-pane-desktop/cpp/src/native_shell.cpp; do
    "$clang_tidy" --config-file="$repository_root/.clang-tidy" --warnings-as-errors='*' "$source" -- -std=c++17 -isysroot "$sdk_path" -I"$repository_root/crates/dual-pane-desktop/cpp/include" -isystem "$generated_headers" -isystem "$qt_headers" -isystem "$qt_core_headers" -F "$qt_libraries" -iframework "$qt_libraries"
done
