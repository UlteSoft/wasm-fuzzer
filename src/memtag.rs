//! uwvm linear-domain v1 instructions and globals v2. No historical wire fallback.
use crate::{Config, generate::Rng};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use wasm_encoder::{Encode, Instruction};
use wasmparser_memtag::{BinaryReader, MemoryType, Parser, Payload, TypeRef};

pub const GLOBALS_NAME: &str = "uwvm.memtag.linear.globals";
pub const WIRE_BASE: u32 = 0x0101_0000;

#[derive(Clone, Copy, Debug)]
pub struct Descriptor {
    pub code: u16,
    pub name: &'static str,
    pub inputs: u8,
    pub outputs: u8,
}
pub const OPERATIONS: [Descriptor; 19] = [
    Descriptor {
        code: 0x20,
        name: "status",
        inputs: 0,
        outputs: 1,
    },
    Descriptor {
        code: 0x21,
        name: "extract",
        inputs: 1,
        outputs: 1,
    },
    Descriptor {
        code: 0x22,
        name: "insert",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x23,
        name: "tagbits",
        inputs: 0,
        outputs: 1,
    },
    Descriptor {
        code: 0x24,
        name: "startbit",
        inputs: 0,
        outputs: 1,
    },
    Descriptor {
        code: 0x25,
        name: "copy",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x26,
        name: "sub",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x27,
        name: "load",
        inputs: 1,
        outputs: 1,
    },
    Descriptor {
        code: 0x28,
        name: "untag",
        inputs: 1,
        outputs: 1,
    },
    Descriptor {
        code: 0x29,
        name: "untagstore",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x2a,
        name: "untagstorez",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x2b,
        name: "store",
        inputs: 2,
        outputs: 0,
    },
    Descriptor {
        code: 0x2c,
        name: "storez",
        inputs: 2,
        outputs: 0,
    },
    Descriptor {
        code: 0x2d,
        name: "random",
        inputs: 1,
        outputs: 1,
    },
    Descriptor {
        code: 0x2e,
        name: "randomstore",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x2f,
        name: "randomstorez",
        inputs: 2,
        outputs: 1,
    },
    Descriptor {
        code: 0x36,
        name: "hint",
        inputs: 3,
        outputs: 1,
    },
    Descriptor {
        code: 0x37,
        name: "hintstore",
        inputs: 4,
        outputs: 1,
    },
    Descriptor {
        code: 0x38,
        name: "hintstorez",
        inputs: 4,
        outputs: 1,
    },
];
pub fn descriptor(code: u16) -> Option<&'static Descriptor> {
    OPERATIONS.iter().find(|op| op.code == code)
}

pub fn instruction(code: u16, memory: u32, out: &mut Vec<u8>) -> Result<()> {
    ensure!(
        descriptor(code).is_some(),
        "reserved/unknown memtag opcode {code:#x}"
    );
    out.push(0xff);
    (WIRE_BASE | u32::from(code)).encode(out);
    memory.encode(out);
    Ok(())
}

pub(crate) fn memories(bytes: &[u8]) -> Result<Vec<MemoryType>> {
    let mut result = vec![];
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::ImportSection(s) => {
                for import in s.into_imports() {
                    if let TypeRef::Memory(m) = import?.ty {
                        result.push(m);
                    }
                }
            }
            Payload::MemorySection(s) => {
                for memory in s {
                    result.push(memory?);
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(crate) fn section(id: u8, bytes: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    (bytes.len() as u32).encode(out);
    out.extend_from_slice(bytes);
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GlobalRange {
    pub memory: u32,
    pub address: u64,
    pub length: u64,
}

pub fn globals_payload(ranges: &[GlobalRange]) -> Vec<u8> {
    let mut bytes = vec![];
    2_u64.encode(&mut bytes);
    (ranges.len() as u64).encode(&mut bytes);
    for r in ranges {
        r.memory.encode(&mut bytes);
        r.address.encode(&mut bytes);
        r.length.encode(&mut bytes);
    }
    bytes
}

/// Also validate allocation granule ownership, not just byte-range overlap.
pub fn validate_globals(bytes: &[u8], memories: &[MemoryType]) -> Result<Vec<GlobalRange>> {
    let mut reader = BinaryReader::new(bytes, 0);
    ensure!(
        reader.read_var_u64()? == 2,
        "memtag globals requires version 2"
    );
    let count = reader.read_var_u64()?;
    ensure!(
        count <= (reader.bytes_remaining() / 3) as u64,
        "impossible memtag globals count"
    );
    let mut ranges = vec![];
    let mut granules = vec![];
    for _ in 0..count {
        let r = GlobalRange {
            memory: reader.read_var_u32()?,
            address: reader.read_var_u64()?,
            length: reader.read_var_u64()?,
        };
        let memory = memories
            .get(r.memory as usize)
            .ok_or_else(|| anyhow::anyhow!("memtag globals memory index out of bounds"))?;
        let end = r
            .address
            .checked_add(r.length)
            .ok_or_else(|| anyhow::anyhow!("memtag globals range overflow"))?;
        let bytes = (memory.initial as u128)
            .checked_shl(memory.page_size_log2.unwrap_or(16))
            .ok_or_else(|| anyhow::anyhow!("invalid memory page size"))?;
        ensure!(
            end as u128 <= bytes,
            "memtag globals range exceeds minimum memory size"
        );
        if r.length > 0 {
            granules.push((r.memory, r.address / 16, (end - 1) / 16));
        }
        ranges.push(r);
    }
    ensure!(reader.eof(), "trailing memtag globals bytes");
    granules.sort_unstable();
    for pair in granules.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        ensure!(
            a.0 != b.0 || a.2 < b.1,
            "overlapping memtag allocation granules"
        );
    }
    Ok(ranges)
}

fn constant(wide: bool, value: u64, out: &mut Vec<u8>) {
    if wide {
        Instruction::I64Const(value as i64).encode(out);
    } else {
        Instruction::I32Const(value as i32).encode(out);
    }
}

fn snippet(op: &Descriptor, memory: u32, wide: bool, mix_numeric: bool, rng: &mut Rng) -> Vec<u8> {
    let mut out = vec![];
    for i in 0..op.inputs {
        let value = match rng.next() % 6 {
            0 => 0,
            1 => 16,
            2 => u64::MAX,
            3 => 1 << if wide { 56 } else { 28 },
            _ => rng.next(),
        };
        constant(wide, value, &mut out);
        // Exercise composition without changing the required operand type.
        if mix_numeric && i == 0 && op.inputs > 0 && rng.next() & 3 == 0 {
            constant(wide, rng.next(), &mut out);
            if wide {
                Instruction::I64Xor.encode(&mut out);
            } else {
                Instruction::I32Xor.encode(&mut out);
            }
        }
    }
    instruction(op.code, memory, &mut out).expect("registered instruction");
    for _ in 0..op.outputs {
        Instruction::Drop.encode(&mut out);
    }
    out
}

/// Insert net-zero typed sequences at random instruction boundaries. Existing
/// section bytes, indices, recursive groups and function signatures are preserved.
pub(crate) fn inject(bytes: &[u8], config: &Config, rng: &mut Rng) -> Result<Vec<u8>> {
    let memories = memories(bytes)?;
    ensure!(
        !memories.is_empty(),
        "memtag generator needs at least one memory"
    );
    let ops: Vec<_> = OPERATIONS
        .iter()
        .filter(|op| {
            config.memtag_operations.is_empty() || config.memtag_operations.contains(&op.code)
        })
        .collect();
    let mut bodies = vec![];
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload? {
            let mut reader = body.get_operators_reader()?;
            let mut points = vec![];
            while !reader.eof() {
                points.push(reader.original_position());
                reader.read()?;
            }
            let mut inserts = vec![];
            let shift = rng.next() as usize % ops.len();
            for n in 0..config.memtag_sequences {
                let mem = rng.next() as usize % memories.len();
                // Cycle covers all enabled operations once the requested sequence count allows.
                let op = ops[(n as usize + shift) % ops.len()];
                let at = points[rng.next() as usize % points.len()];
                inserts.push((
                    at,
                    snippet(
                        op,
                        mem as u32,
                        memories[mem].memory64,
                        config.instructions.numeric,
                        rng,
                    ),
                ));
            }
            inserts.sort_by_key(|(at, _)| *at);
            let mut result = vec![];
            let mut cursor = body.range().start;
            for (at, snippet) in inserts {
                result.extend_from_slice(&bytes[cursor..at]);
                result.extend(snippet);
                cursor = at;
            }
            result.extend_from_slice(&bytes[cursor..body.range().end]);
            bodies.push(result);
        }
    }
    ensure!(
        !bodies.is_empty(),
        "memtag generator needs a defined function"
    );
    let mut code = vec![];
    (bodies.len() as u32).encode(&mut code);
    for body in bodies {
        (body.len() as u32).encode(&mut code);
        code.extend(body);
    }
    let mut out = bytes[..8].to_vec();
    let mut reader = BinaryReader::new(&bytes[8..], 8);
    while !reader.eof() {
        let id = reader.read_u8()?;
        let len = reader.read_var_u32()? as usize;
        let data = reader.read_bytes(len)?;
        section(id, if id == 10 { &code } else { data }, &mut out);
    }
    if config.memtag_globals && config.sections.custom {
        let mut ranges = vec![];
        for (index, memory) in memories.iter().enumerate() {
            let size = (memory.initial as u128) << memory.page_size_log2.unwrap_or(16);
            // Full, disjoint granules with randomized positions. Zero-sized memories
            // and page-size-1 memories intentionally admit empty descriptor lists.
            let slots = (size / 16).min(u64::MAX as u128) as u64;
            if slots > 0 {
                let address = (rng.next() % slots) * 16;
                ranges.push(GlobalRange {
                    memory: index as u32,
                    address,
                    length: 16,
                });
            }
        }
        let mut custom = vec![];
        GLOBALS_NAME.encode(&mut custom);
        custom.extend(globals_payload(&ranges));
        section(0, &custom, &mut out);
    }
    Ok(out)
}

pub(crate) fn check_custom(bytes: &[u8], enabled: bool) -> Result<()> {
    let memories = memories(bytes)?;
    let mut seen = false;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(s) = payload? {
            if s.name() == "memtag.globals" {
                bail!("historical memtag.globals is unsupported; regenerate as globals v2");
            }
            if s.name() == GLOBALS_NAME {
                ensure!(enabled, "memtag globals requires the memtag feature");
                ensure!(!seen, "duplicate memtag globals section");
                seen = true;
                validate_globals(s.data(), &memories)?;
            }
        }
    }
    Ok(())
}

/// Replace each extension instruction with `drop` inputs and typed zero outputs.
/// This preserves typing, not semantics. Original bytes MUST validate first.
/// Official validators cannot validate the private extension's original bytes.
pub fn project(bytes: &[u8], features: &crate::Features) -> Result<Vec<u8>> {
    crate::validate(bytes, features)?;
    let memories = memories(bytes)?;
    let mut bodies = vec![];
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload? {
            let mut ops = body.get_operators_reader()?;
            let mut out = bytes[body.range().start..ops.original_position()].to_vec();
            while !ops.eof() {
                let start = ops.original_position();
                ops.read()?;
                let end = ops.original_position();
                if bytes[start] == 0xff {
                    let mut reader = BinaryReader::new(&bytes[start + 1..end], start + 1);
                    let opcode = reader.read_var_u32()?;
                    let memory = reader.read_var_u32()?;
                    let op = descriptor((opcode & 0xffff) as u16).expect("already validated");
                    for _ in 0..op.inputs {
                        Instruction::Drop.encode(&mut out);
                    }
                    for _ in 0..op.outputs {
                        constant(memories[memory as usize].memory64, 0, &mut out);
                    }
                } else {
                    out.extend_from_slice(&bytes[start..end]);
                }
            }
            bodies.push(out);
        }
    }
    let mut code = vec![];
    (bodies.len() as u32).encode(&mut code);
    for body in bodies {
        (body.len() as u32).encode(&mut code);
        code.extend(body);
    }
    let mut out = bytes[..8].to_vec();
    let mut reader = BinaryReader::new(&bytes[8..], 8);
    while !reader.eof() {
        let id = reader.read_u8()?;
        let len = reader.read_var_u32()? as usize;
        let data = reader.read_bytes(len)?;
        if id == 0 && BinaryReader::new(data, 0).read_string()? == GLOBALS_NAME {
            continue;
        }
        section(id, if id == 10 { &code } else { data }, &mut out);
    }
    let mut standard = features.clone();
    standard.disable(crate::Feature::Memtag);
    crate::validate(&out, &standard)?;
    Ok(out)
}
