#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Use tools/run-in-cgroup.sh bash ../tools/check.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
mkdir -p logs
cargo fmt --manifest-path ../Cargo.toml --all -- --check
cargo clippy --locked --manifest-path ../Cargo.toml --all-targets --no-deps -- -D warnings
cargo test --locked --manifest-path ../Cargo.toml --all-targets --no-run --message-format=json -j 2 > logs/test-build.jsonl
CARGO_MANIFEST_DIR="$repo" python3 - <<'PY'
import json
import subprocess
from pathlib import Path
for line in Path('logs/test-build.jsonl').read_text().splitlines():
    item = json.loads(line)
    if item.get('reason') == 'compiler-artifact' and item.get('executable') and item['profile']['test']:
        subprocess.run([item['executable']], cwd=Path.cwd(), check=True)
PY
cargo build --locked --release --manifest-path ../Cargo.toml
