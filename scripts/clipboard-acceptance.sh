#!/bin/sh
# Guided P4-M5 clipboard acceptance with Finder. Opt-in: not part of `make check`.
#
# It REPLACES the macOS general clipboard and cannot reliably restore it. It
# runs a debug build against a disposable profile and works only inside
# temporary folders. You perform each gesture in Dual Pane and Finder; the
# script stages fixtures and verifies the results (content hashes, symbolic
# link targets, unchanged sources) or asks you to confirm what you saw. Paste
# the printed summary into the P4-M5 acceptance evidence.
#
# The temporary folder is also the profile's Home, so each Browser's Home tab
# lists source and destination. Dual Pane launches with the Left Browser at
# source; open destination in the Right Browser once. Staging empties the
# folders in place, so both Browsers keep showing them between scenarios.
#
#   scripts/clipboard-acceptance.sh              run every scenario
#   scripts/clipboard-acceptance.sh list         list scenario names
#   scripts/clipboard-acceptance.sh NAME...      run the named scenarios
#   scripts/clipboard-acceptance.sh --self-test  check the fixture and verifier
#                                                logic only; no app, no clipboard
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
nfc_name="unicode-caf$(printf '\303\251').txt"
items="plain.txt $nfc_name folder Tool.app link-to-plain dangling-link"
scenarios="copy-to-finder copy-to-finder-after-quit-and-relaunch finder-to-dual-pane finder-to-dual-pane-after-relaunch within-and-across-browsers conflict-and-cancel private-expiry replacement non-file-content cut-is-not-a-finder-move context-menus text-editor-focus cancel-running-paste"
staged_dirs="source destination finder-source finder-out finder-out-quit finder-out-relaunch"
# Files in the cancellation fixture: enough that a Paste runs long enough to cancel.
cancel_files=${CANCEL_FILES:-8000}

work=""
app_pid=""
failed=0

cleanup() {
    if [ -n "$app_pid" ] && kill -0 "$app_pid" 2>/dev/null; then
        kill "$app_pid" 2>/dev/null || true
    fi
    if [ -n "$work" ]; then
        # Keep the summary after the temporary folders are removed.
        if [ -f "$work/results" ]; then
            cp "$work/results" "${TMPDIR:-/tmp}/dual-pane-clipboard-acceptance-last-results.txt"
        fi
        rm -rf "$work"
    fi
}
trap cleanup EXIT

# One line per path under ROOT/NAME: kind, path, then content hash or link target.
manifest() {
    (
        cd "$1"
        find "$2" | LC_ALL=C sort | while IFS= read -r entry; do
            if [ -L "$entry" ]; then
                printf 'L %s -> %s\n' "$entry" "$(readlink "$entry")"
            elif [ -d "$entry" ]; then
                printf 'D %s\n' "$entry"
            else
                printf 'F %s %s\n' "$entry" "$(shasum -a 256 <"$entry" | cut -d' ' -f1)"
            fi
        done
    )
}

make_fixtures() {
    dir=$1
    mkdir -p "$dir/folder/sub" "$dir/Tool.app/Contents/MacOS"
    printf 'plain text\n' >"$dir/plain.txt"
    printf 'precomposed name\n' >"$dir/$nfc_name"
    printf 'inner\n' >"$dir/folder/inner.txt"
    dd if=/dev/urandom of="$dir/folder/sub/deep.bin" bs=1024 count=64 2>/dev/null
    printf '<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict><key>CFBundleName</key><string>Tool</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n' >"$dir/Tool.app/Contents/Info.plist"
    printf '#!/bin/sh\nexit 0\n' >"$dir/Tool.app/Contents/MacOS/tool"
    chmod +x "$dir/Tool.app/Contents/MacOS/tool"
    ln -s plain.txt "$dir/link-to-plain"
    ln -s does-not-exist "$dir/dangling-link"
}

# expect_copied SOURCE_ROOT DESTINATION_ROOT NAME...
expect_copied() {
    from=$1
    to=$2
    shift 2
    ok=0
    for name in "$@"; do
        if [ ! -e "$to/$name" ] && [ ! -L "$to/$name" ]; then
            echo "  missing in destination: $name"
            ok=1
        elif ! manifest "$from" "$name" | diff - "$(manifest_file "$to" "$name")" >/dev/null; then
            echo "  differs from source: $name"
            ok=1
        fi
    done
    return $ok
}

# diff reads the destination manifest from a file so both sides stay portable sh.
manifest_file() {
    out="$work/manifest.$$"
    manifest "$1" "$2" >"$out"
    echo "$out"
}

# expect_unchanged ROOT BASELINE_FILE
expect_unchanged() {
    if manifest_all "$1" | diff - "$2" >/dev/null; then
        return 0
    fi
    echo "  sources changed"
    return 1
}

manifest_all() {
    for name in $items; do
        manifest "$1" "$name"
    done
}

# expect_no_partials ROOT: no operation temporary remains anywhere under ROOT.
expect_no_partials() {
    leftovers=$(find "$1" -name '.dual-pane-*' 2>/dev/null)
    if [ -z "$leftovers" ]; then
        return 0
    fi
    echo "  partial artifacts left: $leftovers"
    return 1
}

# make_many DIR: COUNT one-byte files in folders of 100, for cancellation.
make_many() {
    mkdir -p "$1"
    made=0
    while [ "$made" -lt "$cancel_files" ]; do
        folder="$1/d$((made / 100))"
        mkdir -p "$folder"
        printf x >"$folder/f$made"
        made=$((made + 1))
    done
}

# many_paths ROOT: the sorted paths of ROOT/many, relative to ROOT.
many_paths() {
    (cd "$1" && find many | LC_ALL=C sort)
}

# expect_many_subset SOURCE_ROOT DESTINATION_ROOT: every path a cancelled
# Paste left under DESTINATION_ROOT/many exists in the source, and every file
# there is whole. An absent destination folder is an empty subset.
expect_many_subset() {
    if [ ! -e "$2/many" ]; then
        return 0
    fi
    many_paths "$1" >"$work/many.source"
    many_paths "$2" >"$work/many.destination"
    extra=$(LC_ALL=C comm -13 "$work/many.source" "$work/many.destination")
    broken=$(find "$2/many" -type f ! -size 1c)
    if [ -z "$extra$broken" ]; then
        return 0
    fi
    echo "  not in the source: $extra"
    echo "  incomplete files: $broken"
    return 1
}

# expect_many_unchanged ROOT BASELINE_FILE
expect_many_unchanged() {
    if many_paths "$1" | diff - "$2" >/dev/null && [ -z "$(find "$1/many" -type f ! -size 1c)" ]; then
        return 0
    fi
    echo "  source many changed"
    return 1
}

clipboard_types() {
    osascript -e 'clipboard info' 2>/dev/null || true
}

has_file_url() {
    printf '%s' "$1" | grep -q furl
}

expect_no_file_url() {
    types=$(clipboard_types)
    if has_file_url "$types"; then
        echo "  clipboard types: $types"
        return 1
    fi
    return 0
}

# confirm LABEL QUESTION: records the operator's yes or no.
confirm() {
    printf '%s [y/n] ' "$2"
    read -r reply
    case "$reply" in
    y | Y | yes) record PASS "$1" ;;
    *) record FAIL "$1" ;;
    esac
}

# empty_dir DIR: empties a staged folder in place, so a Browser showing it
# never sees it go missing. Refuses anything outside the temporary folder.
empty_dir() {
    case "$1" in
    "$work"/?*) [ -n "$work" ] ;;
    *) false ;;
    esac || {
        echo "Refusing to empty $1"
        exit 3
    }
    mkdir -p "$1"
    find "$1" -mindepth 1 -delete
}

# expect_absent ROOT NAME...
expect_absent() {
    root=$1
    shift
    ok=0
    for name in "$@"; do
        if [ -e "$root/$name" ] || [ -L "$root/$name" ]; then
            echo "  unexpectedly present: $name"
            ok=1
        fi
    done
    return $ok
}

record() {
    printf '%s  %s\n' "$1" "$2" >>"$work/results"
    echo "  => $1: $2"
    if [ "$1" = FAIL ]; then
        failed=1
    fi
}

# check NAME COMMAND... : records PASS when COMMAND succeeds.
check() {
    label=$1
    shift
    if "$@"; then
        record PASS "$label"
    else
        record FAIL "$label"
    fi
}

pause() {
    printf '%s\n  [Enter to continue] ' "$1"
    read -r _answer
}

launch_app() {
    DUAL_PANE_TEST_PROFILE="$profile" "$repo_root/target/debug/dual-pane-desktop" "$work/source" &
    app_pid=$!
    sleep 2
}

quit_app() {
    pause "Quit Dual Pane with Command+Q, then continue."
    waited=0
    while kill -0 "$app_pid" 2>/dev/null; do
        waited=$((waited + 1))
        if [ "$waited" -gt 30 ]; then
            echo "  Dual Pane is still running; quit it before continuing."
            waited=0
        fi
        sleep 1
    done
    app_pid=""
}

stage() {
    for dir in $staged_dirs; do
        empty_dir "$work/$dir"
    done
    make_fixtures "$work/source"
    make_fixtures "$work/finder-source"
    manifest_all "$work/source" >"$work/source.manifest"
    manifest_all "$work/finder-source" >"$work/finder-source.manifest"
    echo "  Left Browser source:   $work/source"
    echo "  Right Browser target:  $work/destination"
    echo "  Finder source folder:  $work/finder-source"
    echo "  Finder paste target:   $work/finder-out"
    echo "  Dual Pane: the Left Browser starts at source. If the Right Browser is not at"
    echo "  destination, open destination from its Home tab ($work)."
}

scenario_copy_to_finder() {
    stage
    open "$work/finder-out"
    pause "In Dual Pane (Left, $work/source) select plain.txt only and press Command+C. In the Finder window for finder-out press Command+V."
    check "Copy one file, Paste in Finder" expect_copied "$work/source" "$work/finder-out" plain.txt
    pause "Select ALL items in the Left Browser (Command+A) and press Command+C. Paste in the same Finder window; choose Keep Both or Replace if asked."
    check "Copy all item types, Paste in Finder (files, folders, package, symbolic links, precomposed name)" expect_copied "$work/source" "$work/finder-out" $items
    check "Copy leaves Dual Pane sources unchanged" expect_unchanged "$work/source" "$work/source.manifest"
    echo "  Clipboard types after Copy (the first item must list a file URL):"
    osascript -e 'clipboard info' || true
}

scenario_copy_to_finder_after_quit_and_relaunch() {
    stage
    mkdir -p "$work/finder-out-quit" "$work/finder-out-relaunch"
    open "$work/finder-out-quit"
    pause "Select ALL items in the Left Browser ($work/source) and press Command+C. Do not copy anything else."
    quit_app
    pause "Paste into the Finder window finder-out-quit with Command+V (Dual Pane is closed)."
    check "Paste in Finder after Dual Pane quit" expect_copied "$work/source" "$work/finder-out-quit" $items
    launch_app
    open "$work/finder-out-relaunch"
    pause "Dual Pane is running again. Do NOT copy in it. Paste into the Finder window finder-out-relaunch."
    check "Paste in Finder after Dual Pane relaunch" expect_copied "$work/source" "$work/finder-out-relaunch" $items
    check "Sources unchanged" expect_unchanged "$work/source" "$work/source.manifest"
}

scenario_finder_to_dual_pane() {
    stage
    open "$work/finder-source"
    pause "In Finder ($work/finder-source) select plain.txt and press Command+C. In Dual Pane focus the RIGHT Browser at $work/destination and press Command+V."
    check "Finder file, Paste in Dual Pane" expect_copied "$work/finder-source" "$work/destination" plain.txt
    pause "In Finder select ALL items (Command+A) and press Command+C. Paste into the Right Browser; resolve conflicts with Keep Both or Replace."
    check "Finder items of every type, Paste in Dual Pane" expect_copied "$work/finder-source" "$work/destination" $items
    check "Finder sources unchanged" expect_unchanged "$work/finder-source" "$work/finder-source.manifest"
}

scenario_finder_to_dual_pane_after_relaunch() {
    stage
    open "$work/finder-source"
    pause "In Finder select ALL items in $work/finder-source and press Command+C."
    quit_app
    launch_app
    pause "Dual Pane relaunched. Focus the RIGHT Browser at $work/destination and press Command+V."
    check "Finder Copy pasted into relaunched Dual Pane" expect_copied "$work/finder-source" "$work/destination" $items
    check "Finder sources unchanged" expect_unchanged "$work/finder-source" "$work/finder-source.manifest"
}

scenario_within_and_across_browsers() {
    stage
    pause "Left Browser at $work/source, Right at $work/destination. In the LEFT Browser select folder, press Command+C, focus the RIGHT Browser and press Command+V."
    check "Copy across Browsers" expect_copied "$work/source" "$work/destination" folder
    pause "In the LEFT Browser select plain.txt and press Command+X, then Paste in the RIGHT Browser with Command+V. Wait for the operation to finish."
    # finder-source holds an identical fixture, kept as the reference because Cut removes the original.
    check "Cut and Paste across Browsers moves the file" expect_copied "$work/finder-source" "$work/destination" plain.txt
    check "Cut source is gone after Paste" expect_absent "$work/source" plain.txt
    pause "Confirm the cut cue cleared (no muted rows or 'ready to move' count in either Browser), then continue."
    pause "In the RIGHT Browser select folder, press Command+C, then Command+V in that same Browser. Confirm a same-folder Paste is explained and rejected."
    check "Same-folder Paste created nothing new" test "$(ls "$work/destination" | wc -l | tr -d ' ')" = 2
}

scenario_conflict_and_cancel() {
    stage
    printf 'sentinel\n' >"$work/destination/plain.txt"
    pause "In the LEFT Browser copy plain.txt (Command+C) and Paste into the RIGHT Browser. At the conflict decision choose Cancel."
    check "Cancelling at a conflict leaves the destination item unchanged" test "$(cat "$work/destination/plain.txt")" = sentinel
    check "Cancel leaves sources unchanged" expect_unchanged "$work/source" "$work/source.manifest"
}

scenario_private_expiry() {
    stage
    pause "In the LEFT Browser select plain.txt and press Command+X (Cut)."
    quit_app
    launch_app
    pause "Dual Pane relaunched. Focus the RIGHT Browser at $work/destination and press Command+V. A Cut cannot survive relaunch, so Paste must be refused with an explanation."
    check "Expired Cut moved nothing" expect_absent "$work/destination" plain.txt
    check "Expired Cut left the source in place" test -f "$work/source/plain.txt"
    pause "In the LEFT Browser select plain.txt and press Command+C (Copy)."
    quit_app
    launch_app
    pause "Relaunched. Focus the RIGHT Browser at $work/destination and press Command+V. The Finder-compatible representation remains, so this must be a safe Copy."
    check "Copy survives relaunch as a safeguarded external Copy" expect_copied "$work/source" "$work/destination" plain.txt
}

scenario_replacement() {
    stage
    pause "In the LEFT Browser select plain.txt and press Command+C. Do NOT paste yet. Next the script replaces the clipboard with text, as another app would."
    # Separate a Paste made before the replacement from one that ignored it.
    if ! expect_absent "$work/destination" plain.txt >/dev/null; then
        record FAIL "Replacement scenario invalid: plain.txt was pasted before the clipboard was replaced"
        return
    fi
    types=$(clipboard_types)
    echo "  Clipboard types after Dual Pane Copy: $types"
    if ! has_file_url "$types"; then
        record FAIL "Replacement scenario invalid: Dual Pane Copy left no file URL to replace"
        return
    fi
    printf 'replaced by another application' | pbcopy
    types=$(clipboard_types)
    echo "  Clipboard types after replacement: $types"
    if has_file_url "$types"; then
        record FAIL "Replacement left a file URL on the clipboard (script could not replace it)"
    fi
    pause "Press Command+V exactly once in the RIGHT Browser at $work/destination. Paste must be refused with an explanation."
    if ! expect_absent "$work/destination" plain.txt; then
        echo "  destination now holds: $(ls "$work/destination" | tr '\n' ' ')"
    fi
    check "Replacement prevented any Paste" expect_absent "$work/destination" plain.txt
}

scenario_non_file_content() {
    stage
    printf 'just text, not files' | pbcopy
    pause "The clipboard now holds plain text. Focus the RIGHT Browser at $work/destination and press Command+V (or open its context menu). Paste must be unavailable or refused."
    check "Non-file content created nothing" expect_absent "$work/destination" $items
}

scenario_cut_is_not_a_finder_move() {
    stage
    open "$work/finder-out"
    pause "In the LEFT Browser select plain.txt and press Command+X."
    types=$(osascript -e 'clipboard info' 2>/dev/null || true)
    echo "  Clipboard types after Cut: $types"
    if printf '%s' "$types" | grep -q furl; then
        record FAIL "Cut published a file URL"
    else
        record PASS "Cut publishes no file URL"
    fi
    pause "Focus the Finder window finder-out. Edit > Paste Item must be unavailable."
    check "Cut sources stay in place" expect_unchanged "$work/source" "$work/source.manifest"
}

scenario_context_menus() {
    stage
    mkdir "$work/destination/inner"
    printf 'text, not files' | pbcopy
    pause "The clipboard holds text. In the LEFT Browser right-click plain.txt, read the menu, then press Escape to close it."
    confirm "Selected-row menu order" "Did it start with Copy Items, Cut Items, Paste Items, then a separator, then Copy to Other Browser, Move to Other Browser, and Rename Item?"
    confirm "Selected-row menu enablement" "Were Copy Items and Cut Items enabled and Paste Items disabled?"
    pause "Right-click plain.txt again and choose Copy Items."
    check "Copy Items published a file URL" has_file_url "$(clipboard_types)"
    pause "In the RIGHT Browser at $work/destination right-click the empty space below the rows, read the menu, then press Escape."
    confirm "Empty-space menu" "Did it show only Paste Items, a separator, and New Folder, with Paste Items enabled?"
    pause "In the RIGHT Browser right-click the folder row inner and choose Paste Items."
    check "Menu Paste copied into the active folder" expect_copied "$work/source" "$work/destination" plain.txt
    check "Menu Paste ignored the right-clicked folder row" expect_absent "$work/destination/inner" plain.txt
}

scenario_text_editor_focus() {
    stage
    printf 'typed-text' | pbcopy
    pause "Click plain.txt in the LEFT Browser, press Command+F to focus the filter row, press Command+V once, read the filter row, then press Escape."
    confirm "Command+V in the filter row pastes text" "Did the filter row show typed-text?"
    pause "Select plain.txt in the LEFT Browser and press Return to rename it. Press Command+A, then Command+C, then Escape to cancel the rename."
    check "Command+C in the rename editor copies text" test "$(pbpaste)" = plain.txt
    check "Command+C in the rename editor publishes no file URL" expect_no_file_url
    pause "Select plain.txt, press Return, press Command+V once, then Escape to cancel the rename."
    confirm "Command+V in the rename editor pastes text" "Did the rename editor show the pasted text before Escape?"
    check "Editor shortcuts started no file operation" expect_unchanged "$work/source" "$work/source.manifest"
    check "Editor shortcuts pasted nothing into the destination" test -z "$(ls -A "$work/destination")"
}

scenario_cancel_running_paste() {
    stage
    make_many "$work/source/many"
    many_paths "$work/source" >"$work/many.baseline"
    pause "In the LEFT Browser select the folder many ($cancel_files files) and press Command+C. Focus the RIGHT Browser, press Command+V, and click Cancel in its Operation Panel as soon as the panel appears. Wait until the panel shows the result."
    if [ -e "$work/destination/many" ] && many_paths "$work/destination" | diff - "$work/many.baseline" >/dev/null; then
        record FAIL "Cancellation not exercised: the Paste finished before Cancel (rerun with CANCEL_FILES set higher)"
    else
        record PASS "Cancelled Paste stopped before copying every item"
    fi
    check "Cancelled Paste left no partial artifacts" expect_no_partials "$work/destination"
    check "Items a cancelled Paste completed are whole and come from the source" expect_many_subset "$work/source" "$work/destination"
    check "Cancelled Paste left sources unchanged" expect_many_unchanged "$work/source" "$work/many.baseline"
}

run_scenario() {
    echo
    echo "== $1"
    if [ -z "$app_pid" ] || ! kill -0 "$app_pid" 2>/dev/null; then
        launch_app
    fi
    "scenario_$(printf '%s' "$1" | tr '-' '_')"
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-clipboard-self-test.XXXXXX")
    mkdir -p "$work/a" "$work/b"
    make_fixtures "$work/a"
    cp -R "$work/a/." "$work/b/"
    manifest_all "$work/a" >"$work/a.manifest"
    expect_copied "$work/a" "$work/b" $items || { echo "self-test: identical trees must match"; exit 1; }
    expect_unchanged "$work/a" "$work/a.manifest" || { echo "self-test: untouched sources must match"; exit 1; }
    printf 'changed\n' >>"$work/b/folder/inner.txt"
    if expect_copied "$work/a" "$work/b" folder >/dev/null; then echo "self-test: a changed file must be detected"; exit 1; fi
    rm -f "$work/b/link-to-plain"
    if expect_copied "$work/a" "$work/b" link-to-plain >/dev/null; then echo "self-test: a missing link must be detected"; exit 1; fi
    ln -s elsewhere "$work/b/link-to-plain"
    if expect_copied "$work/a" "$work/b" link-to-plain >/dev/null; then echo "self-test: a retargeted link must be detected"; exit 1; fi
    expect_absent "$work/b" never-created || { echo "self-test: absent name must be absent"; exit 1; }
    if expect_absent "$work/b" plain.txt >/dev/null; then echo "self-test: a present name must be reported"; exit 1; fi
    printf 'x' >>"$work/a/plain.txt"
    if expect_unchanged "$work/a" "$work/a.manifest" >/dev/null; then echo "self-test: a changed source must be detected"; exit 1; fi
    empty_dir "$work/b"
    { [ -d "$work/b" ] && [ -z "$(ls -A "$work/b")" ]; } || { echo "self-test: a staged folder must be emptied in place"; exit 1; }
    if (empty_dir "/tmp/outside") >/dev/null; then echo "self-test: a folder outside the work folder must be refused"; exit 1; fi
    cancel_files=250
    make_many "$work/a/many"
    [ "$(find "$work/a/many" -type f | wc -l | tr -d ' ')" = 250 ] || { echo "self-test: the cancellation fixture must hold every file"; exit 1; }
    many_paths "$work/a" >"$work/many.baseline"
    expect_many_unchanged "$work/a" "$work/many.baseline" || { echo "self-test: an untouched fixture must match"; exit 1; }
    expect_many_subset "$work/a" "$work/b" || { echo "self-test: an absent destination is an empty subset"; exit 1; }
    mkdir -p "$work/b/many/d0"
    printf x >"$work/b/many/d0/f0"
    expect_many_subset "$work/a" "$work/b" || { echo "self-test: a completed part must be a subset"; exit 1; }
    expect_no_partials "$work/b" || { echo "self-test: no partials must pass"; exit 1; }
    : >"$work/b/many/d0/f1"
    if expect_many_subset "$work/a" "$work/b" >/dev/null; then echo "self-test: an incomplete file must be detected"; exit 1; fi
    rm "$work/b/many/d0/f1"
    printf x >"$work/b/many/d0/extra"
    if expect_many_subset "$work/a" "$work/b" >/dev/null; then echo "self-test: a path missing from the source must be detected"; exit 1; fi
    : >"$work/b/many/.dual-pane-0-1-2.partial"
    if expect_no_partials "$work/b" >/dev/null; then echo "self-test: a partial artifact must be detected"; exit 1; fi
    printf x >>"$work/a/many/d1/f100"
    if expect_many_unchanged "$work/a" "$work/many.baseline" >/dev/null; then echo "self-test: a changed fixture must be detected"; exit 1; fi
    echo "self-test passed"
}

case "${1:-}" in
--self-test)
    self_test
    exit 0
    ;;
list)
    for name in $scenarios; do echo "$name"; done
    exit 0
    ;;
esac

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

work=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-clipboard-acceptance.XXXXXX")
# The profile is the work folder, so a Home tab lists every staged folder.
profile="$work"
for dir in $staged_dirs; do
    mkdir -p "$work/$dir"
done
cd "$repo_root"
cargo build -q -p dual-pane-desktop
launch_app
pause "Dual Pane opened with the Left Browser at $work/source. In the Right Browser open destination from its Home tab ($work), then continue."

selected=${*:-$scenarios}
for name in $selected; do
    case " $scenarios " in
    *" $name "*) run_scenario "$name" ;;
    *) echo "Unknown scenario: $name (try: list)"; exit 2 ;;
    esac
done

echo
echo "== Summary (paste into the P4-M5 acceptance evidence)"
cat "$work/results"
exit "$failed"
