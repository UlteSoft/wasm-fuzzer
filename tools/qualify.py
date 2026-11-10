#!/usr/bin/env python3
"""One timed gate: exact features, future syntax, long histories and all regressions."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time

from verify import guard


def read_report(path):
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        return {}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpora", type=Path, nargs="+")
    for name in ["generator", "wasm-tools", "official", "uwvm", "uwvm-pure", "wasm-git", "baseline-wasm-git", "syntax-checker", "history-checker", "regression-artifacts", "wasm-git-root", "spec-root", "uwvm-source", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--mutation-corpus", type=Path, action="append", required=True)
    parser.add_argument("--duration", type=float, default=1800)
    parser.add_argument("--heartbeat", type=float, default=600)
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--random-cases", type=int, default=2048)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--quick-ports", action="store_true", help="Smoke tests only; disallowed for the 30-minute gate")
    parser.add_argument("--history-steps", type=int, default=2048)
    parser.add_argument("--history-forks", type=int, default=16)
    parser.add_argument("--replay-queue", type=int, default=128)
    parser.add_argument("--extended-seconds", type=int, default=600)
    args = parser.parse_args()
    assert args.duration >= 0 and args.heartbeat > 0 and args.random_cases > 0
    assert not args.quick_ports or args.duration < 1800
    if args.duration >= 1800:
        assert args.history_steps >= 2048 and args.history_forks >= 16 and args.replay_queue >= 128
        assert args.extended_seconds == 600
    scratch, group = guard()
    output = args.output.resolve()
    assert output.is_relative_to(scratch)
    output.mkdir()
    started = time.monotonic()
    report = {"schema": 1, "started_unix": time.time(), "required_seconds": args.duration,
              "cgroup": str(group), "memory_max": 68719476736, "swap_max": 0,
              "memory_events_before": (group / "memory.events").read_text(), "failures": []}
    core = [os.sys.executable, str(Path(__file__).with_name("cross-test.py")),
            *map(str, args.corpora), "--output", str(output / "core"),
            "--duration", str(args.duration), "--heartbeat", str(args.heartbeat), "--workers", str(args.workers)]
    for name in ["generator", "wasm-tools", "official", "uwvm", "uwvm-pure", "wasm-git"]:
        core += ["--" + name, str(getattr(args, name.replace("-", "_")).resolve())]
    syntax = [os.sys.executable, str(args.syntax_checker.resolve()),
              "--previous", str(args.baseline_wasm_git.resolve()), "--current", str(args.wasm_git.resolve()),
              "--output", str(output / "syntax"), "--duration", str(args.duration),
              "--random-cases", str(args.random_cases), "--seed", str(args.seed), "--heartbeat", str(args.heartbeat)]
    for corpus in args.mutation_corpus:
        syntax += ["--corpus", str(corpus.resolve())]
    if args.quick_ports:
        syntax.append("--quick-ports")
    history = [os.sys.executable, str(args.history_checker.resolve()),
               "--previous", str(args.baseline_wasm_git.resolve()), "--current", str(args.wasm_git.resolve()),
               "--output", str(output / "history"), "--duration", str(args.duration), "--heartbeat", str(args.heartbeat),
               "--steps", str(args.history_steps), "--forks", str(args.history_forks),
               "--replay-queue", str(args.replay_queue), "--seed", str(args.seed)]
    for corpus in args.mutation_corpus:
        history += ["--corpus", str(corpus.resolve())]
    regressions = [os.sys.executable, str(Path(__file__).with_name("regressions.py")),
                   "--artifacts", str(args.regression_artifacts.resolve()),
                   "--wasm-git-root", str(args.wasm_git_root.resolve()),
                   "--spec-root", str(args.spec_root.resolve()), "--uwvm-source", str(args.uwvm_source.resolve()),
                   "--uwvm-bin", str(args.uwvm.resolve()), "--output", str(output / "regressions"),
                   "--extended-seconds", str(args.extended_seconds)]
    processes, logs = {}, []

    def save():
        report["elapsed_seconds"] = time.monotonic() - started
        for name in ["core", "syntax", "history", "regressions"]:
            result = read_report(output / name / "report.json")
            report[name] = {key: result.get(key) for key in ["passed", "elapsed_seconds", "completed_cases", "random_cases", "applications", "counts", "location_counts", "port_cases", "port_counts", "all_ports_covered", "quick_ports", "chain_steps", "forks", "patch_applications", "checkouts", "passed_tests", "active", "all_ignored_executed", "failures"]}
        pending = output / "report.pending"
        pending.write_text(json.dumps(report, indent=2) + "\n")
        pending.replace(output / "report.json")

    try:
        for name, command in [("core", core), ("syntax", syntax), ("history", history), ("regressions", regressions)]:
            log = (output / (name + ".log")).open("w")
            logs.append(log)
            processes[name] = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        next_status, next_heartbeat = 0, args.heartbeat
        while any(process.poll() is None for process in processes.values()):
            for name, process in processes.items():
                if process.poll() not in (None, 0):
                    raise RuntimeError(f"{name} qualification failed with exit {process.returncode}; {output / (name + '.log')}")
            elapsed = time.monotonic() - started
            if elapsed >= next_status or elapsed >= next_heartbeat:
                save()
                label = f"HEARTBEAT {next_heartbeat / 60:g}min" if elapsed >= next_heartbeat else f"STATUS {elapsed:.1f}s"
                print(f"{label} core={report['core']['completed_cases']} ports={report['syntax']['port_cases']} random={report['syntax']['random_cases']} history_checkouts={report['history']['checkouts']} regressions={report['regressions']['passed_tests']} failures=0", flush=True)
                next_status = elapsed + 30
                if elapsed >= next_heartbeat:
                    next_heartbeat += args.heartbeat
            time.sleep(2)
        for name, process in processes.items():
            assert process.wait() == 0, name
        core_result = read_report(output / "core/report.json")
        syntax_result = read_report(output / "syntax/report.json")
        history_result = read_report(output / "history/report.json")
        regression_result = read_report(output / "regressions/report.json")
        assert core_result["passed"] and syntax_result["passed"] and history_result["passed"] and regression_result["passed"]
        assert core_result["elapsed_seconds"] >= args.duration and syntax_result["elapsed_seconds"] >= args.duration
        assert syntax_result["random_cases"] >= args.random_cases
        assert syntax_result["all_ports_covered"]
        assert history_result["elapsed_seconds"] >= args.duration
        assert history_result["chain_steps"] >= args.history_steps and history_result["forks"] >= args.history_forks
        assert history_result["long_forward_replays"] == 2 * args.history_steps
        assert history_result["long_reverse_replays"] == 2 * args.history_steps
        assert history_result["forks_survived_source_removal"] and history_result["two_parent_merge"] and history_result["nested_fork"]
        assert all(history_result[key] >= args.replay_queue for key in ["cherry_pick_steps", "revert_steps", "rebase_steps"])
        assert regression_result["all_ignored_executed"]
        assert {f"section-{i}" for i in range(14)} | {"module-version"} <= set(syntax_result["location_counts"])
        report["memory_events_after"] = (group / "memory.events").read_text()
        report["passed"] = True
        report["qualified_30_minutes"] = args.duration >= 1800 and core_result["qualified_30_minutes"] and not syntax_result["quick_ports"]
        save()
        print(json.dumps(report), flush=True)
    except BaseException as error:
        report["failures"].append(str(error) or type(error).__name__)
        save()
        raise
    finally:
        for process in processes.values():
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
        for process in processes.values():
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        for log in logs:
            log.close()


if __name__ == "__main__":
    main()
