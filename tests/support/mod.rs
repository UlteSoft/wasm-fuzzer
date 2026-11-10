pub fn guard() {
    let scratch =
        std::env::var("WASM_FUZZER_TEST_ROOT").expect("run tests through tools/run-in-cgroup.sh");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".tmp");
    assert_eq!(
        std::path::Path::new(&scratch).canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    let group = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let group = group.lines().find_map(|l| l.strip_prefix("0::")).unwrap();
    let group = std::path::Path::new("/sys/fs/cgroup").join(group.trim_start_matches('/'));
    assert_eq!(
        std::fs::read_to_string(group.join("memory.max"))
            .unwrap()
            .trim(),
        "68719476736"
    );
    assert_eq!(
        std::fs::read_to_string(group.join("memory.swap.max"))
            .unwrap()
            .trim(),
        "0"
    );
    // Cargo normally starts integration tests at the package root. Move the
    // entire test process before any corpus operation; all tests use absolute paths.
    std::env::set_current_dir(root).unwrap();
}
