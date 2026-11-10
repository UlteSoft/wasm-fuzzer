#!/usr/bin/env bash
# Join an EXISTING 64 GiB cgroup; never change its limits or other workloads.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$repo/.tmp/temp" "$repo/.tmp/cargo" "$repo/.tmp/target" "$repo/.tmp/logs"
group=${WASM_FUZZER_CGROUP:-}
container=${WASM_FUZZER_RESOURCE_CONTAINER:-uwvm-sdk-resource-gate-20261024}
if [[ -z $group ]]; then
    id=$(/usr/bin/docker inspect "$container" --format '{{.Id}}')
    group="/sys/fs/cgroup/system.slice/docker-$id.scope"
fi
[[ $(cat "$group/memory.max") == 68719476736 ]]
[[ $(cat "$group/memory.swap.max") == 0 ]]
current="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
if [[ $current != "$group" ]]; then
    if [[ -w $group/cgroup.procs ]]; then
        echo "$$" > "$group/cgroup.procs"
    else
        # A short-lived, networkless helper moves only this waiting shell. It
        # has no host filesystem access except this single cgroup membership file.
        image=$(/usr/bin/docker inspect "$container" --format '{{.Config.Image}}')
        /usr/bin/docker run --rm --network none --read-only --cap-drop ALL \
            --security-opt no-new-privileges --pid host --cgroupns host \
            --mount "type=bind,src=$group/cgroup.procs,dst=/join" \
            --entrypoint /bin/sh "$image" -c 'echo "$1" > /join' sh "$$"
    fi
fi
current="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $current == "$group" ]] || { echo 'Failed to join existing cgroup' >&2; exit 1; }
export TMPDIR="$repo/.tmp/temp" CARGO_HOME="$repo/.tmp/cargo"
export CARGO_TARGET_DIR="$repo/.tmp/target" XDG_CACHE_HOME="$repo/.tmp/cache"
export CARGO_BUILD_JOBS=8 CARGO_INCREMENTAL=0 RUST_TEST_THREADS=8
export PYTHONDONTWRITEBYTECODE=1 WASM_FUZZER_TEST_ROOT="$repo/.tmp"
if [[ -n ${WASM_FUZZER_TOOLCHAIN:-} ]]; then
    export PATH="$WASM_FUZZER_TOOLCHAIN:$PATH"
fi
cd "$repo/.tmp"
umask 022
exec "$@"
