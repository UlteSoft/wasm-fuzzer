use wasm_fuzzer::{
    Config, Feature as F, Features, Profile, config::Engine, generate, inspect, validate,
};

mod support;
use support::guard;

#[test]
fn exact_profiles_and_reproducibility() {
    guard();
    for profile in [
        Profile::WasmMvp,
        Profile::Wasm1,
        Profile::Wasm2,
        Profile::Wasm3,
        Profile::All,
    ] {
        for engine in [Engine::Smith, Engine::Structured] {
            let c = Config {
                features: Features::profile(profile),
                engine,
                ..Default::default()
            };
            for seed in 0..32 {
                let g = generate(&c, seed)
                    .unwrap_or_else(|e| panic!("{profile:?}/{engine:?}/{seed}: {e:#}"));
                validate(&g.bytes, &c.features).unwrap();
                assert_eq!(g.bytes, generate(&c, seed).unwrap().bytes);
                assert_eq!(g.metadata.inventory.imports, 0);
            }
        }
    }
}

#[test]
fn each_proposal_independently_on_mvp() {
    guard();
    for feature in F::ALL {
        let mut c = Config {
            features: Features::profile(Profile::WasmMvp),
            ..Default::default()
        };
        c.features.enable(feature);
        for seed in 0..16 {
            generate(&c, seed).unwrap_or_else(|e| panic!("{} seed {seed}: {e:#}", feature.name()));
        }
    }
}

#[test]
fn feature_swarm() {
    guard();
    let count = std::env::var("WASM_FUZZER_MATRIX_CASES")
        .unwrap_or("512".into())
        .parse::<u64>()
        .unwrap();
    for seed in 0..count {
        let mut c = Config {
            features: Features::profile(Profile::WasmMvp),
            ..Default::default()
        };
        let mut bits = seed.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(19);
        for f in F::ALL {
            if bits & 1 != 0 {
                c.features.enable(f);
            }
            bits >>= 1;
        }
        c.sections.imports = seed % 3 == 0;
        generate(&c, seed).unwrap_or_else(|e| panic!("swarm seed {seed} {:?}: {e:#}", c.features));
    }
}

#[test]
fn section_switches_and_instruction_categories() {
    guard();
    for field in [
        "types",
        "imports",
        "functions",
        "tables",
        "memories",
        "tags",
        "globals",
        "exports",
        "start",
        "elements",
        "data",
        "data_count",
        "custom",
    ] {
        let mut value = serde_json::to_value(Config::default()).unwrap();
        value["sections"][field] = false.into();
        let c: Config = serde_json::from_value(value).unwrap();
        for seed in 0..12 {
            generate(&c, seed).unwrap_or_else(|e| panic!("section {field}, seed {seed}: {e:#}"));
        }
    }
    for field in [
        "numeric",
        "vector",
        "reference",
        "parametric",
        "variable",
        "table",
        "memory",
        "control",
        "aggregate",
    ] {
        let mut value = serde_json::to_value(Config::default()).unwrap();
        value["instructions"][field] = false.into();
        let c: Config = serde_json::from_value(value).unwrap();
        for seed in 0..12 {
            generate(&c, seed)
                .unwrap_or_else(|e| panic!("instruction {field}, seed {seed}: {e:#}"));
        }
    }
}

#[test]
fn dependencies_and_strict_configuration() {
    guard();
    let mut f = Features::profile(Profile::Wasm2);
    f.enable(F::Gc);
    assert!(f.contains(F::FunctionReferences));
    f.disable(F::ReferenceTypes);
    assert!(!f.contains(F::Gc));
    assert!(!f.contains(F::FunctionReferences));
    assert!(!Features::profile(Profile::Wasm3).contains(F::Threads));
    assert!(!Features::profile(Profile::Wasm3).contains(F::Memtag));
    assert!(serde_json::from_str::<Config>(r#"{"unknown":1}"#).is_err());
    let mut c = Config::default();
    c.features.enable(F::Memtag);
    c.sections.memories = false;
    assert!(generate(&c, 0).is_err());
    c.sections.memories = true;
    c.memtag_operations = vec![0x33];
    assert!(generate(&c, 0).is_err());
}

#[test]
fn memtag_all_operations_widths_and_projection() {
    guard();
    let c = Config {
        features: Features::profile(Profile::All),
        engine: Engine::Structured,
        ..Default::default()
    };
    let mut wide = false;
    let mut narrow = false;
    for seed in 0..32 {
        let g = generate(&c, seed).unwrap();
        let inv = inspect(&g.bytes).unwrap();
        wide |= inv.memories64 > 0;
        narrow |= inv.memories32 > 0;
        let ops = inv
            .instructions
            .keys()
            .filter(|n| n.starts_with("Memtag"))
            .count();
        assert_eq!(ops, 19);
        let mut standard = c.features.clone();
        standard.disable(F::Memtag);
        assert!(validate(&g.bytes, &standard).is_err());
        let projection = wasm_fuzzer::memtag::project(&g.bytes, &c.features).unwrap();
        validate(&projection, &standard).unwrap();
    }
    assert!(wide && narrow);
}

#[test]
fn rejected_wrong_feature_and_malformed_memtag() {
    guard();
    let m = wat::parse_str("(module (func v128.const i32x4 0 0 0 0 drop))").unwrap();
    assert!(validate(&m, &Features::profile(Profile::WasmMvp)).is_err());
    assert!(validate(&m, &Features::profile(Profile::Wasm2)).is_ok());
    let c = Config {
        features: Features::profile(Profile::All),
        ..Default::default()
    };
    let good = generate(&c, 0).unwrap().bytes;
    let at = good
        .windows(5)
        .position(|b| b == [0xff, 0xa0, 0x80, 0x84, 0x08])
        .unwrap();
    let mut reserved = good.clone();
    reserved[at + 1] = 0xb3;
    assert!(validate(&reserved, &c.features).is_err());
    let mut domain = good.clone();
    domain[at + 3] = 0x88;
    assert!(validate(&domain, &c.features).is_err());
    let mut index = good.clone();
    index[at + 5] = 127;
    assert!(validate(&index, &c.features).is_err());
    let mut truncated = good.clone();
    truncated.truncate(at + 2);
    assert!(validate(&truncated, &c.features).is_err());
}

#[test]
fn globals_v2_bounds_overlap_and_framing() {
    guard();
    use wasm_fuzzer::memtag::{GlobalRange as R, globals_payload, validate_globals};
    let memory = wasmparser_memtag::MemoryType {
        memory64: false,
        shared: false,
        initial: 1,
        maximum: Some(1),
        page_size_log2: None,
    };
    let good = vec![
        R {
            memory: 0,
            address: 0,
            length: 16,
        },
        R {
            memory: 0,
            address: 16,
            length: 31,
        },
    ];
    assert_eq!(
        validate_globals(&globals_payload(&good), &[memory]).unwrap(),
        good
    );
    for bad in [
        vec![R {
            memory: 1,
            address: 0,
            length: 16,
        }],
        vec![R {
            memory: 0,
            address: 65535,
            length: 2,
        }],
        vec![R {
            memory: 0,
            address: u64::MAX,
            length: 1,
        }],
        vec![
            R {
                memory: 0,
                address: 0,
                length: 1,
            },
            R {
                memory: 0,
                address: 1,
                length: 1,
            },
        ],
    ] {
        assert!(validate_globals(&globals_payload(&bad), &[memory]).is_err());
    }
    let mut trailing = globals_payload(&good);
    trailing.push(0);
    assert!(validate_globals(&trailing, &[memory]).is_err());
    assert!(validate_globals(&[1, 0], &[]).is_err());
    assert!(validate_globals(&[2, 1], &[]).is_err());
    assert!(validate_globals(&[2, 0], &[]).is_ok());
}

#[test]
fn fuel_and_zero_budgets() {
    guard();
    for profile in [Profile::WasmMvp, Profile::Wasm3, Profile::All] {
        let c = Config {
            features: Features::profile(profile),
            fuel: Some(1000),
            ..Default::default()
        };
        for seed in 0..16 {
            generate(&c, seed).unwrap();
        }
    }
    for engine in [Engine::Smith, Engine::Structured] {
        let mut c = Config {
            engine,
            ..Default::default()
        };
        c.limits.types = 0;
        c.limits.functions = 0;
        c.limits.globals = 0;
        c.limits.tables = 0;
        c.limits.memories = 0;
        c.limits.tags = 0;
        c.limits.exports = 0;
        c.limits.imports = 0;
        c.limits.segments = 0;
        c.limits.elements = 0;
        c.limits.instructions = 0;
        for seed in [0, 1, u64::MAX] {
            generate(&c, seed).unwrap();
        }
    }
}
