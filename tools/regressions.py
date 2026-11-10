#!/usr/bin/env python3
"""Run every compiled Rust regression, including explicit extended campaigns."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from verify import guard


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("artifacts", "wasm-git-root", "spec-root", "uwvm-source", "uwvm-bin", "output"):
        parser.add_argument("--" + flag, type=Path, required=True)
    parser.add_argument("--extended-seconds", type=int, default=600)
    args = parser.parse_args()
    assert 1 <= args.extended_seconds <= 600
    scratch, group = guard()
    output = args.output.resolve()
    assert output.is_relative_to(scratch)
    output.mkdir()
    temporary = output / "tmp"
    temporary.mkdir()
    artifacts = json.loads(args.artifacts.read_text())
    hashes = {item["binary"]: hashlib.sha256(Path(item["binary"]).read_bytes()).hexdigest()
              for items in artifacts.values() for item in items}
    assert all(item.get("sha256", hashes[item["binary"]]) == hashes[item["binary"]]
               for items in artifacts.values() for item in items)
    environment = dict(os.environ, TMPDIR=str(temporary), RUST_TEST_THREADS="2", WASM_FUZZER_MATRIX_CASES="5000",
                       WASM_GIT_CC=os.environ.get("WASM_GIT_CC", shutil.which("clang-21") or "clang"))
    cpus = sorted(os.sched_getaffinity(0))[:4]
    started = time.monotonic()
    report = {"schema": 1, "cgroup": str(group), "executables": [], "passed_tests": 0,
              "extended_seconds": args.extended_seconds, "ignored_inventory": {}, "ignored_executed": [], "failures": [], "binary_sha256": hashes}

    def save():
        report["elapsed_seconds"] = time.monotonic() - started
        pending = output / "report.pending"
        pending.write_text(json.dumps(report, indent=2) + "\n")
        pending.replace(output / "report.json")

    def run(label, command, extra=None, limited=True, test=True):
        env = dict(environment, **(extra or {}))
        if limited:
            command = ["taskset", "-c", ",".join(map(str, cpus)), *map(str, command)]
        report["active"] = label
        save()
        log = output / (label + ".log")
        with log.open("w") as stream:
            result = subprocess.run(command, cwd=output, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=1800)
        assert result.returncode == 0, f"{label}: exit {result.returncode}; {log}"
        if test:
            matches = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored;", log.read_text())
            assert matches, (label, log)
            report["passed_tests"] += sum(int(passed) for passed, _ in matches)
        report["executables"].append(label)
        save()
        print(f"PASS regression {label}: total={report['passed_tests']}", flush=True)

    try:
        known = {}
        for project, entries in artifacts.items():
            manifest_root = args.wasm_git_root.resolve() if project == "wasm-git" else Path(__file__).resolve().parents[1]
            for item in entries:
                binary = Path(item["binary"]).resolve()
                assert binary.is_relative_to(scratch)
                label = project + "-" + item["name"]
                ignored = subprocess.check_output([binary, "--list", "--ignored"], cwd=output, env=environment, text=True)
                names = [line[:-6] for line in ignored.splitlines() if line.endswith(": test")]
                if names:
                    report["ignored_inventory"][label] = names
                    assert project == "wasm-git"
                    known[item["name"]] = (label, binary, names)
                run(label, [binary, "--test-threads=2"], {"CARGO_MANIFEST_DIR": str(manifest_root)})
        expected = {"spec_corpus", "uwvm_examples", "uwvm_example_workflows", "audit_fuzz", "memtag_fuzz", "debug_language_formats"}
        assert set(known) == expected, f"review newly ignored tests: {set(known) ^ expected}"
        common = {
            "CARGO_MANIFEST_DIR": str(args.wasm_git_root.resolve()),
            "WASM_GIT_CORPUS_MODE": "fixed-mutations",
            "WASM_GIT_SPEC_ROOT": str(args.spec_root.resolve()), "WASM_GIT_SPEC_VERSION": "3",
            "WASM_GIT_SPEC_REPORT": str(temporary / "official-spec.json"),
            "WASM_GIT_UWVM_EXAMPLES_ROOT": str(args.uwvm_source.resolve()),
            "WASM_GIT_UWVM_CORPUS_REPORT": str(temporary / "uwvm-corpus.json"),
            "WASM_GIT_UWVM_BUILD_ROOT": str(temporary / "uwvm-build"),
            "WASM_GIT_UWVM_WORKFLOW_ROOT": str(temporary / "uwvm-workflows"),
            "WASM_GIT_UWVM_WORKFLOW_REPORT_DIR": str(temporary / "uwvm-workflow-reports"),
            "WASM_GIT_UWVM_BIN": str(args.uwvm_bin.resolve()),
            "WASM_GIT_AUDIT_FUZZ_REPORT_DIR": str(temporary / "audit-fuzz"),
            "WASM_GIT_AUDIT_FUZZ_CASES": "10000",
            "WASM_GIT_MEMTAG_FUZZ_SECONDS": str(args.extended_seconds),
            "WASM_GIT_MEMTAG_FUZZ_OUTPUT": str(temporary / "memtag-fuzz"),
        }
        for target in ["spec_corpus", "uwvm_examples", "uwvm_example_workflows", "audit_fuzz", "memtag_fuzz", "debug_language_formats"]:
            label, binary, names = known[target]
            if target == "debug_language_formats":
                run(label + "-extended", [os.sys.executable, str(args.wasm_git_root.resolve() / "tools/fuzz-debug-languages.py"),
                    "--test-binary", str(binary), "--output", str(temporary / "debug-fuzz"),
                    "--seconds", str(args.extended_seconds)], common, limited=False, test=False)
                report["passed_tests"] += 16
            else:
                run(label + "-extended", [str(binary), "--ignored", "--nocapture", "--test-threads=1"], common, limited=target != "memtag_fuzz")
            report["ignored_executed"] += [label + ":" + name for name in names]
        expected_names = {label + ":" + name for label, names in report["ignored_inventory"].items() for name in names}
        assert expected_names == set(report["ignored_executed"])
        assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == value for path, value in hashes.items())
        report["all_ignored_executed"] = True
        report["passed"] = True
        report["active"] = None
        save()
        print(f"PASS complete regressions: {report['passed_tests']} executions, {len(expected_names)} opt-in tests covered", flush=True)
    except BaseException as error:
        report["failures"].append(repr(error))
        save()
        raise


if __name__ == "__main__":
    main()
