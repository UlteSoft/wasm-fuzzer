#!/usr/bin/env bash
# Build the actual official reference interpreter under the project scratch tree.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Use tools/run-in-cgroup.sh bash ../tools/build-reference.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
command -v ocamlc >/dev/null
command -v dune >/dev/null
command -v menhir >/dev/null
revision=943c5df07e84f11d2b83078964df5812530afbd2
mkdir reference
curl --fail --location --silent --show-error \
    "https://codeload.github.com/WebAssembly/spec/tar.gz/$revision" -o reference/spec.tar.gz
tar -xf reference/spec.tar.gz -C reference
cd "reference/spec-$revision/interpreter"
dune build wasm.exe
cp _build/default/wasm.exe ../../wasm
printf '%s\n' "$revision" > ../../revision.txt
sha256sum ../../wasm > ../../wasm.sha256
echo "Official reference interpreter: $repo/.tmp/reference/wasm"
