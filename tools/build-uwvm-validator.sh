#!/usr/bin/env bash
# Build only the validation adapter, using an existing uwvm2-ros checkout.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Run through tools/run-in-cgroup.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
source_root=$(cd "${1:?Pass the uwvm2-ros source directory}" && pwd)
mkdir -p uwvm-validator
"${CXX:-c++}" -std=c++26 -O2 -MMD -MF uwvm-validator/headers.d \
    -I "$source_root/src" -I "$source_root/third-parties/fast_io/include" \
    -I "$source_root/third-parties/bizwen/include" -I "$source_root/third-parties/boost_unordered/include" \
    "$repo/tools/uwvm-validator.cc" -o uwvm-validator/validate
git -C "$source_root" rev-parse HEAD > uwvm-validator/source-revision.txt
python3 - <<'PY'
import hashlib
import json
from pathlib import Path
import shlex

folder = Path("uwvm-validator")
dependencies = shlex.split((folder / "headers.d").read_text().replace("\\\n", " ").split(":", 1)[1])
hashes = {str(Path(name).resolve()): hashlib.sha256(Path(name).read_bytes()).hexdigest()
          for name in sorted(set(dependencies + [str(folder / "validate")]))}
(folder / "source-sha256.json").write_text(json.dumps(hashes, indent=2) + "\n")
PY
echo "uwvm validation adapter: $repo/.tmp/uwvm-validator/validate"
