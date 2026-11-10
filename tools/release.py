#!/usr/bin/env python3
"""Allowlisted release archives; local smoke tests require the qualification cgroup."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRATCH = Path(os.environ.get("WASM_RELEASE_SCRATCH", ROOT / ".tmp")).resolve()
TARGETS = {
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "armv7-unknown-linux-musleabihf", "riscv64gc-unknown-linux-musl",
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
    "wasm32-wasip1-mvp", "wasm32-wasip1-core2",
}


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]


def smoke(args):
    from verify import guard
    root, _ = guard()
    assert SCRATCH.is_relative_to(root)
    parent = SCRATCH / "release-smoke"
    parent.mkdir(parents=True, exist_ok=True)
    client = Path(args.client).resolve()
    prefix = args.runner_arg or []
    with tempfile.TemporaryDirectory(dir=parent, prefix=args.label + "-") as folder:
        work = Path(folder)

        def run(*options):
            return subprocess.run([*prefix, str(client), *map(str, options)], cwd=work,
                                  check=True, capture_output=True, timeout=180).stdout
        assert version() in run("--version").decode()
        for profile in ["empty", "wasmmvp", "wasm2", "wasm3", "all"]:
            run("generate", "--profile", profile, "--count", "4", "--out-dir", profile)
            for line in (work / profile / "manifest.jsonl").read_text().splitlines():
                item = json.loads(line)
                path = Path(profile) / item["file"]
                run("validate", "--config", Path(profile) / "config.json", path)
                run("replay", path.with_suffix(".json"))
                assert hashlib.sha256((work / path).read_bytes()).hexdigest() == item["sha256"]
        run("matrix", "--scope", "all", "--strength", "triplewise", "--plan-only", "--out-dir", "matrix")
        assert len(json.loads((work / "matrix/matrix.json").read_text())["features"]) == 8886
    print("PASS release smoke:", args.label)


# PE import inspection adapted from wasm-git (Apache-2.0).
def windows_imports(path):
    """Read the actual PE import table; compiler flags alone are not proof."""
    data = path.read_bytes()
    if data[:2] != b"MZ":
        raise RuntimeError(f"not a PE executable: {path}")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise RuntimeError(f"invalid PE header: {path}")
    count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise RuntimeError("release Windows executables must be PE32+")
    sections = optional + optional_size

    def offset(rva):
        for i in range(count):
            size, virtual, raw_size, raw = struct.unpack_from("<4I", data, sections + i * 40 + 8)
            if virtual <= rva < virtual + max(size, raw_size):
                delta = rva - virtual
                if delta >= raw_size or raw + delta >= len(data):
                    break
                return raw + delta
        raise RuntimeError(f"unbacked PE import RVA: {rva}")

    # Import directory is index 1 in the PE32+ optional header.
    rva, size = struct.unpack_from("<2I", data, optional + 112 + 8)
    if not rva or size < 20:
        raise RuntimeError("missing Windows import directory")
    start = offset(rva)
    imports = []
    for pos in range(start, min(start + size, len(data)) - 19, 20):
        descriptor = struct.unpack_from("<5I", data, pos)
        if not any(descriptor):
            return imports
        name = offset(descriptor[3])
        end = data.find(b"\0", name, min(name + 512, len(data)))
        if end < 0:
            raise RuntimeError("unterminated PE import name")
        imports.append(data[name:end].decode("ascii").lower())
    raise RuntimeError("unterminated PE import directory")


def verify_windows_runtime(path):
    imports = windows_imports(path)
    forbidden = [name for name in imports if name.startswith(("vcruntime", "msvcp", "msvcr", "concrt", "api-ms-win-crt-"))
                 or name == "ucrtbase.dll"]
    if forbidden:
        raise RuntimeError(f"Windows release requires an external C/C++ runtime: {path}: {forbidden}")
    print(f"PASS static CRT imports: {path.name}: {', '.join(imports)}", flush=True)


def package(args):
    assert args.label in TARGETS
    client = Path(args.client).resolve()
    if "windows" in args.label:
        verify_windows_runtime(client)
    name = f"wasm-fuzzer-{version()}-{args.label}"
    files = [(client, client.name)]
    for filename in ["LICENSE", "README.md", "THIRD_PARTY.md", "RELEASE_NOTES.md"]:
        files.append((ROOT / filename, filename))
    for crate in ["wasm-smith", "wasmparser-memtag"]:
        for path in sorted((ROOT / "vendor" / crate).glob("LICENSE*")):
            files.append((path, f"licenses/{crate}/{path.name}"))
    for path, _ in files:
        assert path.is_file() and not path.is_symlink(), path
    destination = SCRATCH / "dist"
    destination.mkdir(parents=True, exist_ok=True)
    if "windows" in args.label:
        archive = destination / (name + ".zip")
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as output:
            for path, relative in files: output.write(path, f"{name}/{relative}")
    else:
        archive = destination / (name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as output:
            for path, relative in files: output.add(path, arcname=f"{name}/{relative}", recursive=False)
    print(archive)


def checksums(_args):
    destination = SCRATCH / "dist"
    archives = sorted([*destination.glob("*.tar.gz"), *destination.glob("*.zip")])
    expected = {f"wasm-fuzzer-{version()}-{target}" + (".zip" if "windows" in target else ".tar.gz") for target in TARGETS}
    assert {path.name for path in archives} == expected, "release must contain exactly the ten declared platforms"
    (destination / "SHA256SUMS").write_text("".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in archives))


def inventory(_args):
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).split(b"\0")
    for encoded in filter(None, paths):
        path = os.fsdecode(encoded)
        assert not path.startswith((".tmp/", "target/", "build/", "artifacts/", "corpus/", "reports/")), path
        assert not path.endswith((".wasm", ".log", ".pyc", ".wgp", ".wgb")), path
    print("PASS source-only tracked inventory")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ["smoke", "package"]:
        child = commands.add_parser(name)
        child.add_argument("--client", required=True)
        child.add_argument("--label", required=True)
        if name == "smoke": child.add_argument("--runner-arg", action="append", default=[])
    for name in ["checksums", "inventory"]: commands.add_parser(name)
    args = parser.parse_args()
    {"smoke": smoke, "package": package, "checksums": checksums, "inventory": inventory}[args.command](args)


if __name__ == "__main__":
    main()
