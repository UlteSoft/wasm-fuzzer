use wasm_fuzzer::{
    Config, Feature as F, Features, Profile,
    config::Engine,
    generate,
    matrix::{self, Scope, Strength},
    validate,
};
mod support;

#[test]
fn catalog_covers_every_pinned_core_validator_flag() {
    support::guard();
    let expected = wasmparser::WasmFeatures::all()
        .iter_names()
        .filter(|(name, _)| {
            *name != "COMPONENT_MODEL" && !name.starts_with("CM_") && *name != "CM64"
        })
        .fold(wasmparser::WasmFeatures::empty(), |all, (_, flags)| {
            all | flags
        });
    assert_eq!(
        Features::profile(Profile::AllCore).standard_validator(),
        expected
    );
    assert_eq!(
        Features::profile(Profile::Lime1).standard_validator(),
        wasmparser::WasmFeatures::LIME1
    );
    assert_eq!(Features::profile(Profile::Empty).validator().bits(), 0);
}

#[test]
fn orthogonal_embedding_controls_and_function_references() {
    support::guard();
    for engine in [Engine::Smith, Engine::Structured] {
        for disabled in [
            vec![F::Floats],
            vec![F::GcTypes],
            vec![F::Floats, F::GcTypes],
            vec![F::Gc],
        ] {
            let mut config = Config {
                engine,
                ..Config::default()
            };
            config.features.enable(F::StackSwitching);
            for f in disabled {
                config.features.disable(f);
            }
            for seed in 0..64 {
                generate(&config, seed)
                    .unwrap_or_else(|e| panic!("{engine:?} {seed} {:?}: {e:#}", config.features));
            }
        }
    }
}

#[test]
fn enabled_extensions_have_real_witnesses_and_disabled_rejections() {
    support::guard();
    for feature in [
        F::BulkMemoryOpt,
        F::CallIndirectOverlong,
        F::LegacyExceptions,
        F::SharedEverythingThreads,
        F::MemoryControl,
        F::StackSwitching,
        F::CustomDescriptors,
        F::CompactImports,
        F::FunctionReferences,
    ] {
        let mut config = Config {
            features: Features::profile(Profile::WasmMvp),
            engine: Engine::Structured,
            ..Config::default()
        };
        config.features.enable(feature);
        config.sections.imports = feature == F::CompactImports;
        let generated = generate(&config, 0).unwrap();
        let mut disabled = config.features.clone();
        disabled.disable(feature);
        assert!(
            validate(&generated.bytes, &disabled).is_err(),
            "enabled {feature:?} had no feature-specific bytes"
        );
    }
}

#[test]
fn triplewise_plan_covers_every_feasible_three_way_assignment() {
    support::guard();
    let plan = matrix::plan(Scope::All, Strength::Triplewise).unwrap();
    // Build observed signatures once instead of re-scanning the plan per tuple.
    for a in 0..F::ALL.len() {
        for b in a + 1..F::ALL.len() {
            for c in b + 1..F::ALL.len() {
                let tuple = [F::ALL[a], F::ALL[b], F::ALL[c]];
                let seen = plan.iter().fold(0_u16, |seen, f| {
                    let signature = tuple
                        .iter()
                        .enumerate()
                        .fold(0, |bits, (i, &x)| bits | (u8::from(f.contains(x)) << i));
                    seen | (1 << signature)
                });
                for bits in 0..8 {
                    let mut candidate = Features::profile(Profile::Empty);
                    for (i, &f) in tuple.iter().enumerate() {
                        if bits & (1 << i) != 0 {
                            candidate.enable(f);
                        }
                    }
                    let feasible = tuple
                        .iter()
                        .enumerate()
                        .all(|(i, &f)| candidate.contains(f) == (bits & (1 << i) != 0));
                    if feasible {
                        assert_ne!(seen & (1 << bits), 0, "missing {tuple:?}={bits:03b}");
                    }
                }
            }
        }
    }
}

#[test]
fn all_official_proposal_combinations_are_enumerated() {
    support::guard();
    let plan = matrix::plan(Scope::Official, Strength::Exhaustive).unwrap();
    // Ref -> function-ref -> GC: four states; SIMD -> relaxed: three;
    // ten other independent proposal toggles: 2^10.
    assert_eq!(plan.len(), 4 * 3 * (1 << 10));
    assert!(plan.iter().all(|f| f.0.iter().all(|x| x.official())));
    assert!(matrix::plan(Scope::All, Strength::Exhaustive).is_err());
}
