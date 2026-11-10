#!/usr/bin/env bash
# Independent CLI from exactly the same upstream release as the Rust grammar.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Use tools/run-in-cgroup.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
revision=efeee7cafa5a3dcdddb5aab27bc0c05f279929ed
mkdir -p feature-references
archive=feature-references/wasm-tools.tar.gz
if [[ ! -f $archive ]]; then
    curl -fLsS "https://codeload.github.com/bytecodealliance/wasm-tools/tar.gz/$revision" -o "$archive"
fi
echo "62cb4cc04d787f2983c9ee9dab7810f4fb2bc72484e38f211b48f0b45a4fc73d  $archive" | sha256sum -c -
if [[ ! -d feature-references/wasm-tools-$revision ]]; then
    tar -xf "$archive" -C feature-references
fi
cargo build --locked --release --manifest-path "feature-references/wasm-tools-$revision/Cargo.toml" \
    --no-default-features --features validate
