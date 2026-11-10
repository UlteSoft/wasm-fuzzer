use wasm_encoder::*;
use wasm_fuzzer::{Feature, Features, Profile, memtag, validate};

mod support;

fn module(
    code: u16,
    wide: bool,
    wrong_input: bool,
    wrong_result: bool,
    imported: bool,
    memory_index: u32,
) -> Vec<u8> {
    let op = memtag::descriptor(code).unwrap();
    let mut m = Module::new();
    let mut types = TypeSection::new();
    let results: Vec<_> = (0..op.outputs)
        .map(|_| {
            if wide ^ wrong_result {
                ValType::I64
            } else {
                ValType::I32
            }
        })
        .collect();
    types.ty().function([], results);
    m.section(&types);
    let memory = MemoryType {
        minimum: 1,
        maximum: Some(2),
        memory64: wide,
        shared: false,
        page_size_log2: None,
    };
    if imported {
        let mut s = ImportSection::new();
        s.import("fuzz", "m", EntityType::Memory(memory));
        m.section(&s);
    }
    let mut functions = FunctionSection::new();
    functions.function(0);
    m.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(memory);
    m.section(&memories);
    let mut function = Function::new([]);
    for _ in 0..op.inputs {
        function.instruction(&if wide ^ wrong_input {
            Instruction::I64Const(0)
        } else {
            Instruction::I32Const(0)
        });
    }
    let mut bytes = vec![];
    memtag::instruction(code, memory_index, &mut bytes).unwrap();
    function.raw(bytes);
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    m.section(&code);
    m.finish()
}

#[test]
fn every_extension_signature_checks_stack_width_and_memory_index() {
    support::guard();
    let mut f = Features::profile(Profile::Wasm3);
    f.enable(Feature::Memtag);
    for op in memtag::OPERATIONS {
        for wide in [false, true] {
            for imported in [false, true] {
                for index in 0..=u32::from(imported) {
                    validate(&module(op.code, wide, false, false, imported, index), &f).unwrap();
                    if op.inputs > 0 {
                        assert!(
                            validate(&module(op.code, wide, true, false, imported, index), &f)
                                .is_err(),
                            "input: {}",
                            op.name
                        );
                    }
                    if op.outputs > 0 {
                        assert!(
                            validate(&module(op.code, wide, false, true, imported, index), &f)
                                .is_err(),
                            "result: {}",
                            op.name
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn globals_custom_sections_are_feature_gated_and_unique() {
    support::guard();
    let mut features = Features::profile(Profile::WasmMvp);
    features.enable(Feature::Memtag);
    let mut module = Module::new();
    let payload = memtag::globals_payload(&[]);
    let section = CustomSection {
        name: std::borrow::Cow::Borrowed(memtag::GLOBALS_NAME),
        data: std::borrow::Cow::Borrowed(&payload),
    };
    module.section(&section);
    validate(module.as_slice(), &features).unwrap();
    assert!(validate(module.as_slice(), &Features::profile(Profile::WasmMvp)).is_err());
    module.section(&section);
    assert!(validate(module.as_slice(), &features).is_err());
    let mut old = Module::new();
    old.section(&CustomSection {
        name: std::borrow::Cow::Borrowed("memtag.globals"),
        data: std::borrow::Cow::Borrowed(&[1, 0]),
    });
    assert!(validate(old.as_slice(), &features).is_err());
}
