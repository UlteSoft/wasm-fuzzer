#!/usr/bin/env python3
"""Verify downloaded release archives, then exercise Linux/WASIp1 locally."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import zipfile

from release import TARGETS, verify_windows_runtime
from verify import guard


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--project", choices=["wasm-fuzzer", "wasm-git"], required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--release-script", type=Path, required=True)
    parser.add_argument("--wasm-tools", type=Path, required=True)
    parser.add_argument("--wasmtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root, group = guard()
    output, archives = args.output.resolve(), args.archives.resolve()
    assert output.is_relative_to(root) and archives.is_relative_to(root)
    output.mkdir()
    os.sched_setaffinity(0, sorted(os.sched_getaffinity(0))[:4])
    entries = {}
    for line in (archives / "SHA256SUMS").read_text().splitlines():
        digest, filename = line.split()
        assert filename == Path(filename).name and filename not in entries
        assert len(digest) == 64
        entries[filename] = digest
    expected = {f"{args.project}-{args.version}-{target}" + (".zip" if "windows" in target else ".tar.gz") for target in TARGETS}
    assert set(entries) == expected, "release platform inventory mismatch"
    report = {"project": args.project, "version": args.version, "cgroup": str(group), "assets": []}
    for target in sorted(TARGETS):
        base = f"{args.project}-{args.version}-{target}"
        filename = base + (".zip" if "windows" in target else ".tar.gz")
        archive = archives / filename
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == entries[filename], filename
        if filename.endswith(".zip"):
            with zipfile.ZipFile(archive) as source:
                for name in source.namelist():
                    assert not Path(name).is_absolute() and ".." not in Path(name).parts
                source.extractall(output)
        else:
            with tarfile.open(archive) as source: source.extractall(output, filter="data")
        folder = output / base
        suffix = ".exe" if "windows" in target else ".wasm" if "wasip1" in target else ""
        client = folder / (args.project + suffix)
        server = folder / ("wasm-git-server" + suffix) if args.project == "wasm-git" and "wasip1" not in target else None
        assert (folder / "LICENSE").is_file() and (folder / "README.md").is_file()
        record = {"target": target, "archive_sha256": entries[filename], "binaries": []}
        for binary in [client] + ([server] if server else []):
            data = binary.read_bytes()
            if "linux" in target:
                expected_machine = {"x86_64": 62, "aarch64": 183, "armv7": 40, "riscv64gc": 243}[target.split('-')[0]]
                assert data[:4] == b"\x7fELF" and data[5] == 1
                assert int.from_bytes(data[18:20], "little") == expected_machine
                wide = data[4] == 2
                offset = struct.unpack_from("<Q" if wide else "<I", data, 32 if wide else 28)[0]
                size, count = struct.unpack_from("<HH", data, 54 if wide else 42)
                assert all(struct.unpack_from("<I", data, offset + i * size)[0] != 3 for i in range(count)), "unexpected dynamic ELF interpreter"
            elif "windows" in target:
                pe = struct.unpack_from("<I", data, 0x3c)[0]
                assert struct.unpack_from("<H", data, pe + 4)[0] == (0x8664 if target.startswith("x86_64") else 0xaa64)
                verify_windows_runtime(binary)
            elif "apple" in target:
                assert data[:4] == bytes.fromhex("cffaedfe")
                assert struct.unpack_from("<I", data, 4)[0] == (0x01000007 if target.startswith("x86_64") else 0x0100000c)
            else:
                assert data[:8] == b"\0asm\x01\0\0\0"
                feature = "mvp" if target.endswith("mvp") else "wasm2"
                subprocess.run([args.wasm_tools.resolve(), "validate", "--features=" + feature, binary], check=True)
            record["binaries"].append({"name": binary.name, "sha256": hashlib.sha256(data).hexdigest()})
        command = [os.sys.executable, str(args.release_script.resolve()), "smoke", "--label", target, "--client", str(client)]
        if server: command += ["--server", str(server)]
        if "linux" in target:
            qemu = {"x86_64": "qemu-x86_64", "aarch64": "qemu-aarch64", "armv7": "qemu-arm", "riscv64gc": "qemu-riscv64"}[target.split('-')[0]]
            assert shutil.which(qemu), qemu
            command += ["--runner-arg=" + qemu]
        elif "wasip1" in target:
            command += ["--runner-arg=" + str(args.wasmtime.resolve()), "--runner-arg=run", "--runner-arg=-Ccache=y", "--runner-arg=--dir=.::/"]
        else:
            command = None
        if command:
            environment = dict(os.environ, WASM_RELEASE_SCRATCH=str(output))
            with (output / (target + ".log")).open("w") as log:
                subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, env=environment, check=True, timeout=900)
            record["runtime_smoke"] = "passed"
        else:
            record["runtime_smoke"] = "not_executed_on_linux_host"
        report["assets"].append(record)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print("PASS", args.project, target, record["runtime_smoke"], flush=True)
    report["passed"] = True
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
