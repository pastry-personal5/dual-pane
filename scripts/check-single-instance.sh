#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_root=$(mktemp -d "${TMPDIR:-/tmp}/dual-pane-single-instance.XXXXXX")
runtime_root=$(mktemp -d /tmp/dp-si.XXXXXX)
pids=""
cleanup() {
    for pid in $pids; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    rm -rf "$test_root" "$runtime_root"
}
trap cleanup EXIT HUP INT TERM

binary="$repo_root/target/debug/dual-pane-desktop"
mkdir -p "$runtime_root/main" "$test_root/profile" "$test_root/first" "$test_root/forwarded"
chmod 700 "$runtime_root/main"

cd "$repo_root"
cargo build -q -p dual-pane-desktop

start_instance() {
    profile=$1
    runtime=$2
    exit_ms=$3
    shift 3
    DUAL_PANE_TEST_PROFILE="$profile" TMPDIR="$runtime" QT_QPA_PLATFORM=offscreen DUAL_PANE_INTERNAL_INSTANCE_EXIT_MS="$exit_ms" "$binary" "$@" >"$test_root/process-$start_number.log" 2>&1 &
    started_pid=$!
    pids="$pids $started_pid"
    start_number=$((start_number + 1))
}

wait_for_socket() {
    runtime=$1
    attempts=0
    while [ "$attempts" -lt 100 ]; do
        for socket in "$runtime"/*.s; do
            if [ -S "$socket" ]; then
                found_socket=$socket
                return 0
            fi
        done
        attempts=$((attempts + 1))
        sleep 0.05
    done
    echo "single-instance check: receiver socket did not appear" >&2
    return 1
}

wait_for_saved_tabs() {
    database=$1
    attempts=0
    while [ "$attempts" -lt 100 ]; do
        saved_tabs=$(sqlite3 "$database" 'select count(*) from session_tab;' 2>/dev/null || true)
        case "$saved_tabs" in
            ''|0) ;;
            *) return 0 ;;
        esac
        attempts=$((attempts + 1))
        sleep 0.05
    done
    echo "single-instance check: session was not saved in time" >&2
    return 1
}

wait_for_forwarded_target() {
    database=$1
    attempts=0
    while [ "$attempts" -lt 100 ]; do
        saved_target=$(sqlite3 "$database" "select count(*) from session_tab where instr(location, cast('forwarded' as blob)) > 0;" 2>/dev/null || true)
        [ "$saved_target" = 1 ] && return 0
        attempts=$((attempts + 1))
        sleep 0.05
    done
    echo "single-instance check: forwarded target was not saved in time" >&2
    return 1
}

saved_workspace() {
    sqlite3 "$1" "select 'browser', browser, active_position, active from session_browser order by browser; select 'tab', browser, position, hex(location) from session_tab order by browser, position;"
}

wait_for_one_process() {
    first=$1
    second=$2
    attempts=0
    while [ "$attempts" -lt 60 ]; do
        alive=0
        kill -0 "$first" 2>/dev/null && alive=$((alive + 1))
        kill -0 "$second" 2>/dev/null && alive=$((alive + 1))
        [ "$alive" -eq 1 ] && return 0
        attempts=$((attempts + 1))
        sleep 0.05
    done
    echo "single-instance check: startup race did not elect one receiver" >&2
    return 1
}

start_number=1
start_instance "$test_root/profile" "$runtime_root/main" 6000 "$test_root/first"
receiver_pid=$started_pid
wait_for_socket "$runtime_root/main"
socket=$found_socket

DUAL_PANE_TEST_PROFILE="$test_root/profile" TMPDIR="$runtime_root/main" QT_QPA_PLATFORM=offscreen "$binary" "$test_root/forwarded"
kill -0 "$receiver_pid"

database="$test_root/profile/Library/Application Support/Dual Pane/settings.sqlite3"
wait_for_saved_tabs "$database"
wait_for_forwarded_target "$database"
before=$(saved_workspace "$database")
DUAL_PANE_TEST_PROFILE="$test_root/profile" TMPDIR="$runtime_root/main" QT_QPA_PLATFORM=offscreen "$binary"
after=$(saved_workspace "$database")
[ "$before" = "$after" ]

set +e
DUAL_PANE_TEST_PROFILE="$test_root/profile" TMPDIR="$runtime_root/main" QT_QPA_PLATFORM=offscreen "$binary" "$test_root/missing" >/dev/null 2>&1
bad_status=$?
set -e
[ "$bad_status" -eq 2 ]

wait "$receiver_pid"
pids=$(printf '%s' "$pids" | sed "s/ $receiver_pid//")
[ ! -e "$socket" ]
forwarded_name=$(basename "$test_root/forwarded")
[ "$(sqlite3 "$database" "select count(*) from session_tab where instr(location, cast('$forwarded_name' as blob)) > 0;")" -ge 1 ]

mkdir -p "$runtime_root/new" "$test_root/new-profile"
chmod 700 "$runtime_root/new"
start_instance "$test_root/new-profile" "$runtime_root/new" 4000
registered_pid=$started_pid
wait_for_socket "$runtime_root/new"
start_instance "$test_root/new-profile" "$runtime_root/new" 2000 --new-instance
independent_pid=$started_pid
sleep 0.2
kill -0 "$registered_pid"
kill -0 "$independent_pid"
wait "$independent_pid"
wait "$registered_pid"
pids=$(printf '%s' "$pids" | sed "s/ $independent_pid//; s/ $registered_pid//")

mkdir -p "$runtime_root/race" "$test_root/race-profile"
chmod 700 "$runtime_root/race"
start_instance "$test_root/race-profile" "$runtime_root/race" 4000
race_one=$started_pid
start_instance "$test_root/race-profile" "$runtime_root/race" 4000
race_two=$started_pid
wait_for_one_process "$race_one" "$race_two"
wait "$race_one" 2>/dev/null || true
wait "$race_two" 2>/dev/null || true
pids=$(printf '%s' "$pids" | sed "s/ $race_one//; s/ $race_two//")

mkdir -p "$runtime_root/stale" "$test_root/stale-profile"
chmod 700 "$runtime_root/stale"
start_instance "$test_root/stale-profile" "$runtime_root/stale" 10000
stale_pid=$started_pid
wait_for_socket "$runtime_root/stale"
stale_socket=$found_socket
kill -9 "$stale_pid"
wait "$stale_pid" 2>/dev/null || true
pids=$(printf '%s' "$pids" | sed "s/ $stale_pid//")
[ -S "$stale_socket" ]
start_instance "$test_root/stale-profile" "$runtime_root/stale" 1000
replacement_pid=$started_pid
sleep 0.2
kill -0 "$replacement_pid"
wait "$replacement_pid"
pids=$(printf '%s' "$pids" | sed "s/ $replacement_pid//")
[ ! -e "$stale_socket" ]

mkdir -p "$runtime_root/profiles" "$test_root/profile-a" "$test_root/profile-b"
chmod 700 "$runtime_root/profiles"
start_instance "$test_root/profile-a" "$runtime_root/profiles" 1000
profile_a_pid=$started_pid
start_instance "$test_root/profile-b" "$runtime_root/profiles" 1000
profile_b_pid=$started_pid
sleep 0.2
kill -0 "$profile_a_pid"
kill -0 "$profile_b_pid"
wait "$profile_a_pid"
wait "$profile_b_pid"
pids=$(printf '%s' "$pids" | sed "s/ $profile_a_pid//; s/ $profile_b_pid//")

echo "single-instance check: passed"
