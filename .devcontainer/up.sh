#!/bin/bash
#
# Wrapper around `devcontainer up` that works around it hanging the first time
# a container is created.
#
# When the CLI creates a container it starts a `docker events` listener and
# waits for the new container's start event. With podman that is a race: if the
# container starts before the listener is really up, the event is missed and
# the CLI waits forever. It gets stuck in one very specific place - right after
# printing "Container started", with the container running but before any
# lifecycle command has run. Re-running always works, because the container
# then already exists and the CLI never waits for an event.
#
# This is https://github.com/devcontainers/cli/issues/1236, still open as of
# CLI 0.88.0. Drop this wrapper once it's fixed upstream.
#
# So run it with a watchdog, armed only while the CLI is sitting in exactly
# that state: "Container started" is the last thing it printed, and it has
# printed nothing since for $DEVCONTAINER_UP_IDLE_TIMEOUT seconds. A healthy
# run moves on within a second. Anything else - a long image build, a slow
# postCreateCommand - is never touched, however quiet it gets.

set -euo pipefail

idle_timeout=${DEVCONTAINER_UP_IDLE_TIMEOUT:-30}
max_attempts=${DEVCONTAINER_UP_ATTEMPTS:-3}

# The watchdog needs GNU `tail` and `stat`. Without them, run the CLI directly
# and let the caller deal with the hang if it happens.
if ! tail --pid=$$ -n 0 /dev/null 2>/dev/null || ! stat -c %Y /dev/null >/dev/null 2>&1; then
    exec devcontainer up "$@"
fi

log=$(mktemp)
pid=
tail_pid=

# Kill a process and everything under it. Children have to be collected before
# the parent dies, or they're reparented to init and we lose track of them.
kill_tree() {
    local signal=$1 parent=$2 child
    for child in $(ps -o pid= --ppid "$parent" 2>/dev/null); do
        kill_tree "$signal" "$child"
    done
    kill "-$signal" "$parent" 2>/dev/null || true
}

stop_up() {
    [ -n "$pid" ] || return 0
    kill_tree TERM "$pid"
    for _ in 1 2 3 4 5; do
        kill -0 "$pid" 2>/dev/null || break
        sleep 1
    done
    kill_tree KILL "$pid"
}

# shellcheck disable=SC2329 # invoked by the INT/TERM trap below
interrupted() {
    stop_up
    rm -f "$log"
    exit 130
}

trap interrupted INT TERM
trap 'rm -f "$log"' EXIT

# True while the CLI is stuck waiting for a start event it will never see.
is_stuck() {
    [ "$(($(date +%s) - $(stat -c %Y "$log")))" -ge "$idle_timeout" ] &&
        tail -n 1 "$log" | grep -q 'Container started$'
}

for attempt in $(seq 1 "$max_attempts"); do
    : >"$log"

    devcontainer up "$@" >"$log" 2>&1 &
    pid=$!

    tail -n +1 -f --pid="$pid" "$log" &
    tail_pid=$!

    stuck=0
    while kill -0 "$pid" 2>/dev/null; do
        sleep 1
        if is_stuck; then
            stuck=1
            stop_up
            break
        fi
    done

    status=0
    wait "$pid" 2>/dev/null || status=$?
    wait "$tail_pid" 2>/dev/null || true
    pid=

    if [ "$stuck" -eq 0 ]; then
        exit "$status"
    fi

    if [ "$attempt" -lt "$max_attempts" ]; then
        echo "devcontainer up is stuck waiting for the container start event," \
            "retrying (attempt $((attempt + 1))/${max_attempts})" >&2
    fi
done

trap - EXIT
echo "devcontainer up did not get past starting the container after" \
    "${max_attempts} attempts. Output from the last attempt is in $log" >&2
exit 1
