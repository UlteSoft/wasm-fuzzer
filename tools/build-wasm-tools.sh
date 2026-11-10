#!/usr/bin/env bash
# Independent CLI from exactly the same upstream release as the Rust grammar.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
[[ $PWD == "$repo/.tmp" ]] || { echo 'Use tools/run-in-cgroup.sh' >&2; exit 1; }
group="/sys/fs/cgroup$(awk -F:: '$1 == "0" {print $2}' /proc/self/cgroup)"
[[ $(cat "$group/memory.max") == 68719476736 && $(cat "$group/memory.swap.max") == 0 ]]
revision=7a2fa422cdf9aeeaf488a2acac8bc94a078bfa32
mkdir -p feature-references
archive=feature-references/wasm-tools-$revision.tar.gz
if [[ ! -f $archive ]]; then
    curl -fLsS "https://codeload.github.com/bytecodealliance/wasm-tools/tar.gz/$revision" -o "$archive"
fi
echo "51003014f50d31f753e86541eecb54ca2d2ccd8e5028f0a2ecfff6265527742f  $archive" | sha256sum -c -
if [[ ! -d feature-references/wasm-tools-$revision ]]; then
    tar -xf "$archive" -C feature-references
fi
cargo build --locked --release --manifest-path "feature-references/wasm-tools-$revision/Cargo.toml" \
    --no-default-features --features validate
