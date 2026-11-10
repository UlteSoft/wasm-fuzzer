#!/usr/bin/env python3
"""Compile paired regressions without executing tests from source checkouts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

from verify import guard


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wasm-git-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    scratch, _ = guard()
    output = args.output.resolve()
    assert output.is_relative_to(scratch) and not output.exists()
    logs = output.with_suffix(".logs")
    logs.mkdir()
    os.sched_setaffinity(0, sorted(os.sched_getaffinity(0))[:4])
    environment = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    roots = {"wasm-git": args.wasm_git_root.resolve(), "wasm-fuzzer": Path(__file__).resolve().parents[1]}
    artifacts = {}
    for project, root in roots.items():
        path = logs / (project + ".jsonl")
        with path.open("w") as stdout, (logs / (project + ".log")).open("w") as stderr:
            subprocess.run(["cargo", "test", "--locked", "--manifest-path", str(root / "Cargo.toml"),
                            "--all-targets", "--all-features", "--no-run", "--message-format=json", "-j", "2"],
                           cwd=scratch, env=environment, stdout=stdout, stderr=stderr, check=True)
        records = [json.loads(line) for line in path.read_text().splitlines()]
        artifacts[project] = [{"name": item["target"]["name"], "binary": item["executable"],
                               "sha256": hashlib.sha256(Path(item["executable"]).read_bytes()).hexdigest()}
                              for item in records if item.get("reason") == "compiler-artifact"
                              and item.get("executable") and item["profile"]["test"]]
        print(project, len(artifacts[project]), "test executables", flush=True)
    output.write_text(json.dumps(artifacts, indent=2) + "\n")


if __name__ == "__main__":
    main()
