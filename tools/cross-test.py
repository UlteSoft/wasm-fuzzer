#!/usr/bin/env python3
"""Timed, original-byte cross qualification. Always run through run-in-cgroup.sh.

The spec interpreter covers Core 3. Experimental Core proposals are checked by
the pinned, unmodified wasm-tools CLI. Unsupported consumer profiles are counted
separately, never as passes. Every original is validated and replayed by wasm-git.
"""
import argparse
from collections import Counter, deque
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import random
import subprocess
import tempfile
import time

from verify import FEATURE_FLAGS, guard, replay, sha

EXPERIMENTAL = {
    "threads", "wide-arithmetic", "custom-page-sizes", "legacy-exceptions",
    "shared-everything-threads", "memory-control", "stack-switching",
    "custom-descriptors", "compact-imports",
    "acquire-release-atomics", "multibyte-array-access",
}
UWVM_UNSUPPORTED = EXPERIMENTAL - {"threads"}


def fields(module):
    assert module["standard"]["validation"] == "valid", module["standard"]
    for diagnostic in module.get("diagnostics", []):
        assert diagnostic["code"] not in {"operand_projection", "type_projection", "declaration_projection"}, diagnostic

    def visit(regions):
        for region in regions:
            children = region["children"]
            path = region["path"]
            if re.search(r"/operator\[\d+\]$", path) or re.fullmatch(r"type\[\d+\]/type\[\d+\]", path):
                assert children, f"missing physical fields: {path}"
                cursor = region["span"]["start"]
                for child in children:
                    assert child["span"]["start"] == cursor, child
                    cursor = child["span"]["end"]
                assert cursor == region["span"]["end"], region
            visit(children)
    for section in module["sections"]:
        visit(section["regions"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpora", type=Path, nargs="+")
    for name in ["generator", "wasm-tools", "official", "uwvm", "uwvm-pure", "wasm-git", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--duration", type=float, default=1800, help="minimum seconds of active cross checks")
    parser.add_argument("--heartbeat", type=float, default=600)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--limit", type=int, help="development smoke only; prevents a qualification claim")
    args = parser.parse_args()
    root, group = guard()
    assert args.duration >= 0 and args.heartbeat > 0 and 1 <= args.workers <= 16
    output = args.output.resolve()
    assert output.is_relative_to(root)
    output.mkdir()
    tools = {name: getattr(args, name.replace("-", "_")).resolve()
             for name in ["generator", "wasm-tools", "official", "uwvm", "uwvm-pure", "wasm-git"]}
    started = time.monotonic()
    report = {"format": 2, "started_unix": time.time(), "required_seconds": args.duration,
              "cgroup": str(group), "memory_max": 68719476736, "swap_max": 0,
              "memory_events_before": (group / "memory.events").read_text(),
              "tools": {name: {"path": str(path), "sha256": sha(path)} for name, path in tools.items()},
              "feature_policy": "wasm-tools exact flags; wasm-git exact flags and maximal audit decoding",
              "uwvm_mapping": "bulk-memory-opt and explicit multi-memory data segments map to bulk-memory; bulk-memory explicit table-zero segments map to multiple-tables; call-indirect-overlong maps to reference-types; GC initializers map to extended-const; function-references return_call_ref maps to tail-call; floats/gc-types are checked by the exact wasm-tools gate",
              "completed_cases": 0, "completed_cycles": 0, "failures": [], "counts": {}}
    entries = []
    configurations = set()
    for requested in args.corpora:
        requested = requested.resolve()
        assert requested.is_relative_to(root)
        corpora = [requested]
        if (requested / "matrix.json").exists():
            matrix = json.loads((requested / "matrix.json").read_text())
            corpora = [requested / p for p in matrix["corpora"]]
        for corpus in corpora:
            config = corpus / "config.json"
            features = set(json.loads(config.read_text())["features"])
            configurations.add(tuple(sorted(features)))
            previous = None
            for line in (corpus / "manifest.jsonl").read_text().splitlines():
                item = json.loads(line)
                source = corpus / item["file"]
                entries.append((config, features, source, previous, item))
                previous = source
    random.Random(0).shuffle(entries)
    if args.limit:
        entries = entries[:args.limit]
    assert entries
    report["unique_cases"] = len(entries)
    report["feature_configurations"] = len(configurations)
    report["development_limit"] = args.limit
    empty = output / "empty.wasm"
    empty.write_bytes(b"\0asm\x01\0\0\0")

    def command(work, label, argv, success=True):
        result = subprocess.run(list(map(str, argv)), cwd=work, stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=45)
        if success and result.returncode != 0:
            (work / (label + ".stdout")).write_bytes(result.stdout)
            (work / (label + ".stderr")).write_bytes(result.stderr)
            raise RuntimeError(f"{label}: exit {result.returncode}; {work}")
        assert result.returncode >= 0, f"{label}: crashed {result.returncode}"
        return result

    # A well-framed but ill-typed body proves each gate actually validates.
    invalid = output / "invalid-control.wasm"
    invalid.write_bytes(bytes.fromhex("0061736d010000000105016000017f030201000a040102000b"))
    for name, argv in [
        ("official", [tools["official"], "-d", invalid]),
        ("wasm-tools", [tools["wasm-tools"], "validate", invalid]),
        ("uwvm", [tools["uwvm"], "--mode", "validation", "--run", invalid]),
        ("uwvm-pure", [tools["uwvm-pure"], "--mode", "validation", "--run", invalid]),
        ("wasm-git", [tools["wasm-git"], "validate", invalid]),
    ]:
        assert command(output, name, argv, False).returncode > 0, f"{name} accepted invalid control"

    def case(entry):
        config, features, source, previous, item = entry
        work = Path(tempfile.mkdtemp(dir=output, prefix="case-"))
        record = {"file": str(source), "seed": item["seed"], "sha256": item["sha256"]}
        try:
            assert sha(source) == item["sha256"]
            metadata = json.loads(source.with_suffix(".json").read_text())
            command(work, "fuzzer-validate", [tools["generator"], "validate", "--config", config, source])
            command(work, "seed-replay", [tools["generator"], "replay", source.with_suffix(".json")])
            command(work, "git-validate", [tools["wasm-git"], "--threads", "1", "validate", "--config", config, source])
            inspected = command(work, "inspect", [tools["wasm-git"], "--threads", "1", "inspect", source, "--json"])
            try:
                fields(json.loads(inspected.stdout)["audit"])
            except Exception:
                (work / "inspect.json").write_bytes(inspected.stdout)
                raise
            projected = source
            if "memtag" in features:
                projected = work / "projection.wasm"
                command(work, "project", [tools["generator"], "project-memtag", "--config", config, source, "--output", projected])
                if any(name.startswith("Memtag") for name in metadata["inventory"]["instructions"]):
                    assert command(work, "private-original", [tools["wasm-tools"], "validate", "--features=all", source], False).returncode > 0
                    record["private_original"] = "unmodified_validator_rejected"
            selected = ["mutable-global" if f == "mutable-globals" else f for f in sorted(features - {"memtag"})]
            command(work, "wasm-tools", [tools["wasm-tools"], "validate", "--features=-all," + ",".join(selected), projected])
            record["wasm_tools"] = "typed_projection" if projected != source else "original"
            if features & EXPERIMENTAL:
                record["official"] = "unsupported_profile"
            else:
                command(work, "official", [tools["official"], "-d", projected])
                record["official"] = "typed_projection" if projected != source else "original"
            if features & UWVM_UNSUPPORTED:
                record["uwvm"] = "unsupported_profile"
            else:
                enabled = {n for f in features for n in FEATURE_FLAGS.get(f, [])}
                if "gc" in features: enabled.add("extended-const")
                if features & {"bulk-memory-opt", "multi-memory"}: enabled.add("bulk-memory")
                if "call-indirect-overlong" in features: enabled.add("reference-types")
                if "function-references" in features: enabled.add("tail-call")
                if "bulk-memory" in features: enabled.add("multiple-tables")
                flags = ["--wasm-feature-" + ("enable-" if f in enabled else "disable-") + f
                         for f in sorted({n for names in FEATURE_FLAGS.values() for n in names})]
                if "memtag" not in features: flags.append("--wasm-feature-disable-memtag-extension")
                tool = "uwvm-pure" if metadata["inventory"]["imports"] else "uwvm"
                command(work, "uwvm", [tools[tool], "--mode", "validation", *flags, "--run", source])
                record["uwvm"] = tool + "_original"
            previous = previous or empty
            for label, old, new in [("forward", previous, source), ("reverse", source, previous)]:
                result = command(work, label + "-audit", [tools["wasm-git"], "--threads", "1", "diff", old, new, "--format", "json", "--compact"])
                audit = json.loads(result.stdout)["audit"]
                try:
                    assert audit["old"]["standard"]["validation"] == "valid"
                    assert audit["new"]["standard"]["validation"] == "valid"
                    replay(old.read_bytes(), new.read_bytes(), audit["edits"])
                except Exception:
                    (work / (label + "-audit.json")).write_bytes(result.stdout)
                    raise
                patch = work / (label + ".wgp")
                restored = work / (label + ".wasm")
                command(work, label + "-patch", [tools["wasm-git"], "--threads", "1", "patch", old, new, "--output", patch])
                command(work, label + "-apply", [tools["wasm-git"], "--threads", "1", "apply", old, patch, "--output", restored])
                assert restored.read_bytes() == new.read_bytes(), "serialized patch byte mismatch"
            record["passed"] = True
            import shutil
            shutil.rmtree(work)
        except Exception as error:
            record.update(passed=False, error=repr(error), evidence=str(work))
            (work / "failure.json").write_text(json.dumps(record, indent=2))
        return record

    counts = Counter()
    next_heartbeat = args.heartbeat
    next_status = 0

    def save():
        report["elapsed_seconds"] = time.monotonic() - started
        report["counts"] = dict(counts)
        pending = output / "report.pending"
        pending.write_text(json.dumps(report, indent=2) + "\n")
        pending.replace(output / "report.json")

    with (output / "cases.jsonl").open("w") as cases, ThreadPoolExecutor(max_workers=args.workers) as pool:
        while True:
            remaining = iter(entries)
            cut_short = False
            pending = deque(pool.submit(case, entry) for _, entry in zip(range(args.workers * 2), remaining))
            while pending:
                record = pending.popleft().result()
                cases.write(json.dumps(record) + "\n")
                report["completed_cases"] += 1
                for gate in ["official", "uwvm", "wasm_tools"]:
                    if gate in record: counts[gate + ":" + record[gate]] += 1
                if not record["passed"]:
                    for future in pending: future.cancel()
                    report["failures"].append(record)
                    save()
                    raise RuntimeError(f"cross check failed: {record}")
                elapsed = time.monotonic() - started
                if elapsed >= next_status:
                    cases.flush()
                    save()
                    print(f"STATUS {elapsed:.1f}s cases={report['completed_cases']} failures=0", flush=True)
                    next_status = elapsed + 30
                if elapsed >= next_heartbeat:
                    print(f"HEARTBEAT {next_heartbeat / 60:g}min cases={report['completed_cases']} failures=0", flush=True)
                    next_heartbeat += args.heartbeat
                entry = next(remaining, None)
                if entry is not None: pending.append(pool.submit(case, entry))
                if elapsed >= args.duration and report["completed_cycles"] >= 1:
                    remaining = iter(())
                    cut_short = True
            if not cut_short: report["completed_cycles"] += 1
            if time.monotonic() - started >= args.duration: break
    assert all(sha(path) == report["tools"][name]["sha256"] for name, path in tools.items()), "tool binary changed during qualification"
    report["memory_events_after"] = (group / "memory.events").read_text()
    report["finished_unix"] = time.time()
    report["passed"] = True
    report["qualified_30_minutes"] = args.duration >= 1800 and not args.limit
    save()
    print(json.dumps({key: report[key] for key in ["elapsed_seconds", "completed_cases", "unique_cases", "feature_configurations", "counts", "qualified_30_minutes"]}), flush=True)


if __name__ == "__main__":
    main()
