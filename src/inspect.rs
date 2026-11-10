use crate::{Feature, Features, memtag};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wasmparser_memtag::{Parser, Payload, Validator};

pub fn validate(bytes: &[u8], features: &Features) -> Result<()> {
    features.check()?;
    ensure!(
        bytes.starts_with(b"\0asm\x01\0\0\0"),
        "expected a Core Wasm module, not a component or WAT"
    );
    Validator::new_with_features(features.validator()).validate_all(bytes)?;
    memtag::check_custom(bytes, features.contains(Feature::Memtag))?;
    // For standard modules also use the unmodified upstream validator.
    if !features.contains(Feature::Memtag) {
        wasmparser::Validator::new_with_features(features.standard_validator())
            .validate_all(bytes)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub sections: BTreeMap<u8, usize>,
    pub instructions: BTreeMap<String, usize>,
    pub defined_functions: usize,
    pub imports: usize,
    pub memories32: usize,
    pub memories64: usize,
    pub shared_memories: usize,
    pub tables32: usize,
    pub tables64: usize,
    pub custom_sections: Vec<String>,
}

/// Structural inventory (call validate first when checking untrusted inputs).
pub fn inspect(bytes: &[u8]) -> Result<Inventory> {
    let mut inv = Inventory::default();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload?;
        if let Some((id, _)) = payload.as_section() {
            *inv.sections.entry(id).or_default() += 1;
        }
        match payload {
            Payload::ImportSection(s) => {
                for i in s.into_imports() {
                    i?;
                    inv.imports += 1;
                }
            }
            Payload::CustomSection(s) => inv.custom_sections.push(s.name().into()),
            Payload::TableSection(s) => {
                for t in s {
                    if t?.ty.table64 {
                        inv.tables64 += 1;
                    } else {
                        inv.tables32 += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                inv.defined_functions += 1;
                let mut ops = body.get_operators_reader()?;
                while !ops.eof() {
                    let name = format!("{:?}", ops.read()?);
                    let name = name.split([' ', '{']).next().unwrap().to_owned();
                    *inv.instructions.entry(name).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    for memory in memtag::memories(bytes)? {
        if memory.memory64 {
            inv.memories64 += 1;
        } else {
            inv.memories32 += 1;
        }
        inv.shared_memories += usize::from(memory.shared);
    }
    Ok(inv)
}
