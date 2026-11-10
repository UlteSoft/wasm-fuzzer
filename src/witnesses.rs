//! Structured, randomized coverage complements wasm-smith's general generator.
//! In particular wasm-smith couples function references to GC; this path does not.
use crate::{Config, Feature as F, generate::Rng};
use anyhow::Result;
use std::borrow::Cow;
use wasm_encoder::{CustomSection, Module};

pub(crate) fn module(config: &Config, rng: &mut Rng) -> Result<Vec<u8>> {
    let s = &config.sections;
    let l = &config.limits;
    let i = &config.instructions;
    let f = &config.features;
    let funcs = s.types && s.functions && l.types > 0 && l.functions > 0;
    let imported = funcs && s.imports && l.imports > 0 && l.functions >= 2;
    let leaf = funcs && l.functions - usize::from(imported) >= 2;
    let gc = s.types && l.types >= 3 && f.contains(F::Gc) && f.contains(F::GcTypes);
    let continuation =
        funcs && l.types >= 4 && f.contains(F::StackSwitching) && f.contains(F::GcTypes);
    let descriptors = gc && l.types >= 6 && f.contains(F::CustomDescriptors);
    let count = if !s.memories {
        0
    } else {
        l.memories
            .min(if f.contains(F::MultiMemory) { 2 } else { 1 })
    };
    let wide: Vec<_> = (0..count)
        .map(|n| f.contains(F::Memory64) && (n as u64).wrapping_add(rng.next()) & 1 == 0)
        .collect();
    let shared = f.contains(F::Threads) && rng.next() & 1 == 0;
    let table = s.tables && l.tables > 0;
    let table64 = table && f.contains(F::Memory64) && rng.next() & 1 == 0;
    let table_ty = if table64 { "i64" } else { "i32" };
    let table_size = l.table_elements.min(8);
    let tag = s.tags && l.tags > 0 && s.types && l.types > 0 && f.contains(F::Exceptions);
    let global = s.globals && l.globals > 0;
    let elem = s.elements && l.segments > 0 && l.elements > 0 && leaf && table;
    let data = s.data && l.segments > 0 && count > 0;
    let passive = data && s.data_count && f.contains(F::BulkMemory);
    let declared = s.elements
        && l.segments > usize::from(elem)
        && l.elements > 0
        && f.contains(F::ReferenceTypes)
        && f.contains(F::BulkMemory)
        && leaf;
    let mut wat = String::from("(module\n");
    if s.types && l.types > 0 {
        wat.push_str("(type $v (func))\n");
    }
    if gc {
        wat.push_str(
            "(rec (type $s (sub (struct (field (mut i32)))))) (type $a (array (mut i16)))\n",
        );
    }
    if continuation {
        wat.push_str("(type $c (cont $v))\n");
    }
    if descriptors {
        wat.push_str("(rec (type $described (descriptor $descriptor) (struct)) (type $descriptor (describes $described) (struct)))\n");
    }
    if imported {
        wat.push_str("(import \"fuzz\" \"function\" (func $imported (type $v)))\n");
    }
    for (n, is64) in wide.iter().enumerate() {
        let addr = if *is64 { "i64" } else { "" };
        let tiny = f.contains(F::CustomPageSizes) && rng.next() & 1 == 0;
        let page = if tiny { "(pagesize 1)" } else { "" };
        let size = if tiny {
            l.memory_bytes.min(128)
        } else {
            (l.memory_bytes / 65536).min(2)
        };
        let share = if shared { "shared" } else { "" };
        wat.push_str(&format!(
            "(memory $m{n} {addr} {size} {size} {share} {page})\n"
        ));
    }
    if table {
        wat.push_str(&format!(
            "(table $t {} {table_size} {table_size} funcref)\n",
            if table64 { "i64" } else { "" }
        ));
    }
    if tag {
        wat.push_str("(tag $e (type $v))\n");
    }
    if global {
        let value = rng.next() as i32;
        let init = if f.contains(F::ExtendedConst) {
            format!("i32.const {value} i32.const 7 i32.add")
        } else {
            format!("i32.const {value}")
        };
        wat.push_str(&format!("(global $g (mut i32) {init})\n"));
    }
    let shared_global = global && l.globals > 1 && f.contains(F::SharedEverythingThreads);
    if shared_global {
        wat.push_str("(global $shared (shared mut i32) (i32.const 0))\n");
    }
    if leaf {
        wat.push_str("(func $leaf (type $v) nop)\n");
    }
    if funcs {
        wat.push_str("(func $run (type $v) (local $x i32)\n");
        if i.numeric {
            for _ in 0..(l.instructions / 8).clamp(1, 12) {
                let a = rng.next() as i32;
                let b = rng.next() as i32;
                let op =
                    ["add", "sub", "mul", "xor", "rotl", "lt_s", "div_u"][rng.next() as usize % 7];
                wat.push_str(&format!("i32.const {a} i32.const {b} i32.{op} drop\n"));
            }
            if f.contains(F::SignExtension) {
                wat.push_str("i32.const 255 i32.extend8_s drop\n");
            }
            if f.contains(F::SaturatingFloatToInt) && f.contains(F::Floats) {
                wat.push_str("f64.const nan i64.trunc_sat_f64_u drop\n");
            }
            if f.contains(F::WideArithmetic) {
                wat.push_str("i64.const 1 i64.const -1 i64.mul_wide_u drop drop\n");
            }
        }
        if i.control {
            wat.push_str("block $done loop $again i32.const 0 br_if $again br $done end end\n");
            wat.push_str("i32.const 1 if nop else nop end\n");
            if f.contains(F::MultiValue) {
                wat.push_str("block (result i32 i64) i32.const 1 i64.const -1 end drop drop\n");
            }
            if leaf {
                wat.push_str("call $leaf\n");
            }
            if tag {
                wat.push_str("block $caught try_table (catch $e $caught) throw $e end end\n");
                if f.contains(F::LegacyExceptions) {
                    wat.push_str("try throw $e catch $e nop catch_all nop end\n");
                }
            }
            if continuation {
                wat.push_str("ref.null $v cont.new $c cont.bind $c $c resume $c\n");
                if tag {
                    wat.push_str("suspend $e\n");
                }
            }
        }
        if i.variable {
            wat.push_str("i32.const 7 local.set $x local.get $x drop\n");
            if global {
                wat.push_str("global.get $g global.set $g\n");
            }
            if shared_global {
                wat.push_str("global.atomic.get seq_cst $shared drop\n");
            }
        }
        if i.reference && f.contains(F::ReferenceTypes) {
            wat.push_str(if f.contains(F::GcTypes) {
                "ref.null extern ref.is_null drop\n"
            } else {
                "ref.null func ref.is_null drop\n"
            });
        }
        if i.control && i.reference && f.contains(F::FunctionReferences) && (declared || elem) {
            wat.push_str("ref.func $leaf call_ref $v\n");
        }
        if i.aggregate && gc {
            wat.push_str("struct.new_default $s struct.get $s 0 drop\n");
            wat.push_str("i32.const 17 ref.i31 i31.get_s drop\n");
            wat.push_str("i32.const 8 array.new_default $a array.len drop\n");
            wat.push_str("ref.null any ref.test (ref null $s) drop\n");
            if descriptors {
                wat.push_str("struct.new_default $descriptor struct.new_desc $described ref.get_desc $described drop\n");
            }
        }
        if i.vector && f.contains(F::Simd) {
            wat.push_str("v128.const i32x4 1 2 3 4 i32x4.abs drop\n");
            if f.contains(F::RelaxedSimd) {
                wat.push_str("v128.const i32x4 1 2 3 4 v128.const i32x4 4 3 2 1 i8x16.relaxed_swizzle drop\n");
            }
        }
        for (n, is64) in wide.iter().enumerate() {
            let ty = if *is64 { "i64" } else { "i32" };
            if i.memory {
                wat.push_str(&format!("{ty}.const 0 i32.const {} i32.store $m{n}\n{ty}.const 0 i32.load $m{n} drop\nmemory.size $m{n} drop\n", rng.next() as i32));
                if f.contains(F::BulkMemoryOpt) {
                    wat.push_str(&format!(
                        "{ty}.const 0 i32.const 0 {ty}.const 0 memory.fill $m{n}\n"
                    ));
                }
                if f.contains(F::MemoryControl) {
                    wat.push_str(&format!("{ty}.const 0 {ty}.const 0 memory.discard $m{n}\n"));
                }
                if shared {
                    wat.push_str(&format!(
                        "{ty}.const 0 i32.atomic.load $m{n} drop atomic.fence\n"
                    ));
                }
                if passive && n == 0 {
                    wat.push_str(&format!(
                        "{ty}.const 0 i32.const 0 i32.const 0 memory.init $m0 $d data.drop $d\n"
                    ));
                }
            }
        }
        if i.table && table && f.contains(F::ReferenceTypes) {
            wat.push_str(&format!(
                "{table_ty}.const 0 table.get $t drop table.size $t drop\n"
            ));
        }
        if i.control && leaf && elem {
            wat.push_str(&format!("{table_ty}.const 0 call_indirect $t (type $v)\n"));
        }
        if i.control && leaf && f.contains(F::TailCall) {
            wat.push_str("return_call $leaf\n");
        }
        wat.push_str(")\n");
    }
    if s.exports && l.exports > 0 {
        let mut exports = vec![];
        if funcs {
            exports.push("(export \"run\" (func $run))");
        }
        if count > 0 {
            exports.push("(export \"memory\" (memory $m0))");
        }
        if table {
            exports.push("(export \"table\" (table $t))");
        }
        if tag {
            exports.push("(export \"exception\" (tag $e))");
        }
        if global && f.contains(F::MutableGlobals) {
            exports.push("(export \"global\" (global $g))");
        }
        for e in exports.into_iter().take(l.exports) {
            wat.push_str(e);
        }
    }
    if s.start && funcs && rng.next() & 1 == 0 {
        wat.push_str("(start $run)\n");
    }
    if elem {
        wat.push_str(&format!("(elem ({table_ty}.const 0) func $leaf)\n"));
    }
    if declared {
        wat.push_str("(elem declare func $leaf)\n");
    }
    if data {
        if passive {
            wat.push_str("(data $d \"\\00\\ffWasm\")\n");
        } else {
            wat.push_str(&format!(
                "(data ({}.const 0) \"\\00\\ffWasm\")\n",
                if wide[0] { "i64" } else { "i32" }
            ));
        }
    }
    wat.push(')');
    let mut bytes =
        wat::parse_str(&wat).map_err(|e| anyhow::anyhow!("structured encoder: {e}\n{wat}"))?;
    // WAT adds a name section; honor the custom-section switch by removing it.
    if !s.custom {
        let mut out = bytes[..8].to_vec();
        let mut reader = wasmparser_memtag::BinaryReader::new(&bytes[8..], 8);
        while !reader.eof() {
            let id = reader.read_u8()?;
            let len = reader.read_var_u32()? as usize;
            let data = reader.read_bytes(len)?;
            if id != 0 {
                crate::memtag::section(id, data, &mut out);
            }
        }
        bytes = out;
    } else {
        let payload = rng.next().to_le_bytes();
        let mut m = Module::new();
        m.section(&CustomSection {
            name: Cow::Borrowed("wasm-fuzzer.entropy"),
            data: Cow::Borrowed(&payload),
        });
        bytes.extend_from_slice(&m.finish()[8..]);
    }
    Ok(bytes)
}
