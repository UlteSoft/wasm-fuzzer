use std::{fs, path::PathBuf, process::Command};
mod support;

#[test]
fn cli_corpus_replay_rejection_and_no_overwrite() {
    support::guard();
    let root =
        PathBuf::from(std::env::var("WASM_FUZZER_TEST_ROOT").expect("use tools/run-in-cgroup.sh"));
    std::env::set_current_dir(&root).unwrap();
    let bin = env!("CARGO_BIN_EXE_wasm-fuzzer");
    let directory = root.join(format!("cli-{}", std::process::id()));
    let output = directory.to_str().unwrap();
    let invoke = |args: &[&str]| {
        Command::new(bin)
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap()
    };
    let generated = invoke(&[
        "generate",
        "--profile",
        "wasm2.0",
        "--enable",
        "gc,memtag",
        "--seed",
        "42",
        "--count",
        "4",
        "--out-dir",
        output,
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(!invoke(&["generate", "--out-dir", output]).status.success());
    let wasm = directory.join("000000000000002a.wasm");
    let meta = wasm.with_extension("json");
    assert!(invoke(&["replay", meta.to_str().unwrap()]).status.success());
    assert!(
        !invoke(&["validate", "--profile", "wasmmvp", wasm.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        invoke(&[
            "validate",
            "--profile",
            "wasm2",
            "--enable",
            "gc,memtag",
            wasm.to_str().unwrap()
        ])
        .status
        .success()
    );
    let mut bytes = fs::read(&wasm).unwrap();
    bytes.push(0);
    fs::write(&wasm, bytes).unwrap();
    assert!(!invoke(&["replay", meta.to_str().unwrap()]).status.success());
    assert!(
        !invoke(&["generate", "--enable", "unknown", "--out-dir", output])
            .status
            .success()
    );
    assert!(
        !invoke(&[
            "generate",
            "--seed",
            "18446744073709551615",
            "--count",
            "2",
            "--out-dir",
            output
        ])
        .status
        .success()
    );
    assert!(invoke(&["features"]).status.success());
}
