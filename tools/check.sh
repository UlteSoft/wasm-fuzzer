#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Use tools/run-in-cgroup.sh bash ../tools/check.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
mkdir -p logs
cargo fmt --manifest-path ../Cargo.toml --all -- --check
cargo clippy --locked --manifest-path ../Cargo.toml --all-targets --no-deps -- -D warnings
cargo test --locked --manifest-path ../Cargo.toml --all-targets
cargo build --locked --release --manifest-path ../Cargo.toml
