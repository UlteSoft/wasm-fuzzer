#!/usr/bin/env python3
"""Three independent acceptance gates. Run via run-in-cgroup.sh, from .tmp.

Official means WebAssembly/spec's reference interpreter (`wasm -d`), not a
wasmparser alias. Memtag originals must be rejected by it; a typed projection
is an additional check, never a claim of validating private instructions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def guard():
    root = Path(__file__).resolve().parents[1] / ".tmp"
    assert Path.cwd().resolve().is_relative_to(root), "work in wasm-fuzzer/.tmp"
    group = Path("/sys/fs/cgroup") / Path("/proc/self/cgroup").read_text().split("0::", 1)[1].strip().lstrip("/")
    assert (group / "memory.max").read_text().strip() == "68719476736"
    assert (group / "memory.swap.max").read_text().strip() == "0"
    return root, group


def replay(old, new, edits):
    """Reconstruct using audit's hex bytes; verify every unchanged byte too."""
    a = b = 0
    result = bytearray()
    for edit in edits:
        x, y = edit["old_span"], edit["new_span"]
        assert a <= x["start"] <= x["end"] <= len(old)
        assert b <= y["start"] <= y["end"] <= len(new)
        assert old[a:x["start"]] == new[b:y["start"]]
        assert old[x["start"]:x["end"]] == bytes.fromhex(edit["old_hex"])
        assert new[y["start"]:y["end"]] == bytes.fromhex(edit["new_hex"])
        result.extend(old[a:x["start"]])
        result.extend(bytes.fromhex(edit["new_hex"]))
        a, b = x["end"], y["end"]
    assert old[a:] == new[b:]
    result.extend(old[a:])
    assert bytes(result) == new, "audit did not replay 100% of bytes"


FEATURE_FLAGS = {
    "bulk-memory": ["bulk-memory"], "exceptions": ["exceptions"],
    "extended-const": ["extended-const"], "function-references": ["function-references", "table-initializer"],
    "gc": ["gc"], "memory64": ["memory64", "table64"],
    "multi-memory": ["multi-memory"], "multi-value": ["multi-value"],
    "reference-types": ["reference-types", "multiple-tables", "table-instructions"],
    "relaxed-simd": ["relaxed-simd"], "saturating-float-to-int": ["nontrapping-float-to-int"],
    "sign-extension": ["sign-extension"], "simd": ["simd"], "tail-call": ["tail-call"],
    "threads": ["threads"],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpora", type=Path, nargs="+")
    parser.add_argument("--generator", type=Path, required=True)
    parser.add_argument("--official", type=Path, required=True)
    parser.add_argument("--uwvm", type=Path, required=True)
    parser.add_argument("--uwvm-pure", type=Path,
                        help="uwvm production validation adapter for modules with unresolved imports")
    parser.add_argument("--wasm-git", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--uwvm-strict-flags", action="store_true",
                        help="do not map GC local-global initializers to uwvm's extended-const switch")
    args = parser.parse_args()
    root, group = guard()
    assert args.output.resolve().is_relative_to(root)
    args.output.mkdir()  # never replace earlier evidence
    tools = {name: getattr(args, name.replace("-", "_")).resolve() for name in ["generator", "official", "uwvm", "wasm-git"]}
    if args.uwvm_pure:
        tools["uwvm-pure"] = args.uwvm_pure.resolve()
    report = {"format": 1, "started_unix": time.time(), "cgroup": str(group),
              "memory_max": 68719476736, "swap_max": 0,
              "tools": {k: {"path": str(v), "sha256": sha(v)} for k, v in tools.items()},
              "feature_mappings": [], "cases": [], "failures": []}

    def run(label, command, ok=True):
        stdout, stderr = args.output / (label + ".out"), args.output / (label + ".err")
        with stdout.open("wb") as out, stderr.open("wb") as err:
            p = subprocess.run([str(v) for v in command], stdin=subprocess.DEVNULL,
                               stdout=out, stderr=err, timeout=args.timeout)
        if ok:
            assert p.returncode == 0, f"{label}: exit {p.returncode}; see {stderr} and {stdout}"
        return p.returncode, stdout

    # Invalid control proves validation has not accidentally become parse-only.
    invalid = args.output / "invalid-control.wasm"
    invalid.write_bytes(bytes.fromhex("0061736d010000000105016000017f030201000a040102000b"))
    assert run("official-invalid-control", [tools["official"], "-d", invalid], False)[0] > 0
    assert run("uwvm-invalid-control", [tools["uwvm"], "--mode", "validation", "--run", invalid], False)[0] > 0
    if "uwvm-pure" in tools:
        assert run("uwvm-pure-invalid-control", [tools["uwvm-pure"], "--mode", "validation", "--run", invalid], False)[0] == 1
        imported_invalid = args.output / "imported-invalid-control.wasm"
        # Function 0 is imported; function 1 promises an i32 but returns nothing.
        imported_invalid.write_bytes(bytes.fromhex(
            "0061736d010000000108026000006000017f020701016d01660000030201010a040102000b"))
        assert run("official-imported-invalid-control", [tools["official"], "-d", imported_invalid], False)[0] > 0
        assert run("uwvm-pure-imported-invalid-control", [tools["uwvm-pure"], "--mode", "validation", "--run", imported_invalid], False)[0] == 1

    for corpus_number, corpus in enumerate(args.corpora):
        config = corpus / "config.json"
        features = set(json.loads(config.read_text())["features"])
        unsupported = features & {"threads", "wide-arithmetic", "custom-page-sizes"}
        assert not unsupported, f"official Core 3 reference does not implement these additional proposals: {unsupported}"
        # uwvm intentionally rejects mixing version presets with individual flags.
        all_flags = {n for names in FEATURE_FLAGS.values() for n in names}
        enabled = {n for f in features for n in FEATURE_FLAGS.get(f, [])}
        mapping_note = None
        if "gc" in features and "extended-const" not in features and not args.uwvm_strict_flags:
            # wasmparser gates local immutable global.get initializers on GC;
            # uwvm's parser/wasm/standard/wasm1p1/features/types.h gates them
            # on extended-const. The exact standard gate is still checked first.
            enabled.add("extended-const")
            mapping_note = "GC local-global initializers require uwvm extended-const; exact standard feature validation remains unchanged"
        flags = ["--wasm-feature-" + ("enable-" if n in enabled else "disable-") + n for n in sorted(all_flags)]
        if "memtag" not in features:
            flags.append("--wasm-feature-disable-memtag-extension")
        report["feature_mappings"].append({"corpus": str(corpus), "features": sorted(features),
                                           "uwvm_flags": flags, "note": mapping_note})
        previous = args.output / f"empty-{corpus_number}.wasm"
        previous.write_bytes(b"\0asm\x01\0\0\0")
        for case_number, line in enumerate((corpus / "manifest.jsonl").read_text().splitlines()):
            entry = json.loads(line)
            source = corpus / entry["file"]
            label = f"{corpus_number:02}-{case_number:05}"
            case = {"file": str(source), "sha256": sha(source), "seed": entry["seed"]}
            try:
                assert sha(source) == entry["sha256"]
                run(label + "-feature-validation", [tools["generator"], "validate", "--config", config, source])
                run(label + "-seed-replay", [tools["generator"], "replay", source.with_suffix(".json")])
                if "memtag" in features:
                    code, _ = run(label + "-official-original", [tools["official"], "-d", source], False)
                    assert code > 0, "official interpreter accepted private opcodes or crashed"
                    projected = args.output / (label + "-projection.wasm")
                    run(label + "-project", [tools["generator"], "project-memtag", "--config", config, source, "--output", projected])
                    run(label + "-official-projection", [tools["official"], "-d", projected])
                    case["official"] = "private original rejected; typed standard projection validated"
                else:
                    run(label + "-official", [tools["official"], "-d", source])
                    case["official"] = "original bytes validated"
                metadata = json.loads(source.with_suffix(".json").read_text())
                uwvm_tool = "uwvm-pure" if metadata["inventory"]["imports"] and "uwvm-pure" in tools else "uwvm"
                run(label + "-uwvm", [tools[uwvm_tool], "--mode", "validation", *flags, "--run", source])
                case["uwvm"] = "original bytes validated using " + uwvm_tool
                for direction, old, new in [("forward", previous, source), ("reverse", source, previous)]:
                    key = label + "-" + direction
                    _, audit_path = run(key + "-audit", [tools["wasm-git"], "diff", old, new, "--format", "json", "--compact"])
                    audit = json.loads(audit_path.read_text())["audit"]
                    assert audit["old"]["standard"]["validation"] == "valid"
                    assert audit["new"]["standard"]["validation"] == "valid"
                    replay(old.read_bytes(), new.read_bytes(), audit["edits"])
                    patch = args.output / (key + ".patch")
                    applied = args.output / (key + "-applied.wasm")
                    run(key + "-patch", [tools["wasm-git"], "patch", old, new, "--output", patch])
                    run(key + "-apply", [tools["wasm-git"], "apply", old, patch, "--output", applied])
                    assert applied.read_bytes() == new.read_bytes(), "serialized patch replay mismatch"
                case["wasm_git"] = "100% byte replay: audit and serialized patch, forward and reverse"
                case["passed"] = True
            except (AssertionError, subprocess.TimeoutExpired, KeyError, ValueError) as error:
                case["passed"] = False
                case["error"] = str(error)
                report["failures"].append(case)
            report["cases"].append(case)
            previous = source
            (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
            print(f"{label} {'PASS' if case['passed'] else 'FAIL'} {source}", flush=True)
    report["finished_unix"] = time.time()
    report["passed"] = not report["failures"]
    report["memory_events"] = (group / "memory.events").read_text()
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{len(report['cases'])} cases, {len(report['failures'])} failures")
    raise SystemExit(bool(report["failures"]))


if __name__ == "__main__":
    main()
