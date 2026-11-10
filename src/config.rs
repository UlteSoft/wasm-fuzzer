use crate::features::{Feature as F, Features};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use wasm_smith::{InstructionKind as K, InstructionKinds};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Mixed,
    Smith,
    Structured,
}

/// Toggles for every Core section. Function and code are a coupled pair;
/// data-count is emitted when passive-data instructions need it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sections {
    pub types: bool,
    pub imports: bool,
    pub functions: bool,
    pub tables: bool,
    pub memories: bool,
    pub tags: bool,
    pub globals: bool,
    pub exports: bool,
    pub start: bool,
    pub elements: bool,
    pub data: bool,
    pub data_count: bool,
    pub custom: bool,
}
impl Default for Sections {
    fn default() -> Self {
        Self {
            types: true,
            imports: false,
            functions: true,
            tables: true,
            memories: true,
            tags: true,
            globals: true,
            exports: true,
            start: true,
            elements: true,
            data: true,
            data_count: true,
            custom: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Instructions {
    pub numeric: bool,
    pub vector: bool,
    pub reference: bool,
    pub parametric: bool,
    pub variable: bool,
    pub table: bool,
    pub memory: bool,
    pub control: bool,
    pub aggregate: bool,
}
impl Default for Instructions {
    fn default() -> Self {
        Self {
            numeric: true,
            vector: true,
            reference: true,
            parametric: true,
            variable: true,
            table: true,
            memory: true,
            control: true,
            aggregate: true,
        }
    }
}
impl Instructions {
    fn smith(&self) -> InstructionKinds {
        let pairs = [
            (self.numeric, K::Numeric),
            (self.vector, K::Vector),
            (self.reference, K::Reference),
            (self.parametric, K::Parametric),
            (self.variable, K::Variable),
            (self.table, K::Table),
            (self.memory, K::Memory),
            (self.control, K::Control),
            (self.aggregate, K::Aggregate),
        ];
        InstructionKinds::new(
            &pairs
                .into_iter()
                .filter_map(|(on, k)| on.then_some(k))
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub types: usize,
    pub functions: usize,
    pub globals: usize,
    pub tables: usize,
    pub memories: usize,
    pub tags: usize,
    pub imports: usize,
    pub exports: usize,
    pub segments: usize,
    pub elements: usize,
    pub instructions: usize,
    pub memory_bytes: u64,
    pub table_elements: u64,
    pub module_bytes: usize,
    pub entropy_bytes: usize,
    pub attempts: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            types: 32,
            functions: 16,
            globals: 16,
            tables: 4,
            memories: 4,
            tags: 8,
            imports: 12,
            exports: 32,
            segments: 8,
            elements: 32,
            instructions: 128,
            memory_bytes: 16 * 65536,
            table_elements: 256,
            module_bytes: 4 * 1024 * 1024,
            entropy_bytes: 65536,
            attempts: 32,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub features: Features,
    pub engine: Engine,
    pub sections: Sections,
    pub instructions: Instructions,
    pub limits: Limits,
    /// Number of randomized net-zero memtag snippets per defined function.
    pub memtag_sequences: u32,
    pub memtag_globals: bool,
    /// If nonempty, generate only these executable low-16-bit operation IDs.
    pub memtag_operations: Vec<u16>,
    pub canonicalize_nans: bool,
    /// Bounded loops/recursion via wasm-smith's fuel global. Traps remain possible.
    pub fuel: Option<u32>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            features: Features::default(),
            engine: Engine::Mixed,
            sections: Sections::default(),
            instructions: Instructions::default(),
            limits: Limits::default(),
            memtag_sequences: 24,
            memtag_globals: true,
            memtag_operations: vec![],
            canonicalize_nans: false,
            fuel: None,
        }
    }
}
impl Config {
    pub fn check(&self) -> Result<()> {
        self.features.check()?;
        let l = &self.limits;
        ensure!(
            (1..=1024).contains(&l.attempts),
            "attempts must be 1..=1024"
        );
        ensure!(
            (1024..=16 * 1024 * 1024).contains(&l.entropy_bytes),
            "entropy_bytes must be 1 KiB..=16 MiB"
        );
        ensure!(
            (8..=64 * 1024 * 1024).contains(&l.module_bytes),
            "module_bytes must be 8..=64 MiB"
        );
        for (name, value) in [
            ("types", l.types),
            ("functions", l.functions),
            ("globals", l.globals),
            ("tables", l.tables),
            ("memories", l.memories),
            ("tags", l.tags),
            ("imports", l.imports),
            ("exports", l.exports),
            ("segments", l.segments),
            ("elements", l.elements),
            ("instructions", l.instructions),
        ] {
            ensure!(value <= 100_000, "{name} exceeds 100000");
        }
        ensure!(
            self.memtag_sequences <= 4096,
            "memtag_sequences exceeds 4096"
        );
        ensure!(l.memory_bytes <= 1_u64 << 48, "memory_bytes exceeds 2^48");
        ensure!(
            l.table_elements <= u32::MAX as u64,
            "table_elements exceeds u32::MAX"
        );
        if self.features.contains(F::Memtag) {
            ensure!(
                self.sections.memories && l.memories > 0,
                "memtag requires memory generation"
            );
            ensure!(
                self.sections.functions && self.sections.types && l.functions > 0 && l.types > 0,
                "memtag requires types and defined functions"
            );
            ensure!(
                self.memtag_sequences > 0,
                "memtag_sequences must be nonzero with memtag"
            );
        }
        for op in &self.memtag_operations {
            ensure!(
                crate::memtag::descriptor(*op).is_some(),
                "unsupported/reserved memtag operation 0x{op:x}"
            );
        }
        if self.fuel.is_some() {
            ensure!(
                self.sections.globals && l.globals > 0,
                "fuel requires globals"
            );
            ensure!(
                self.engine != Engine::Structured,
                "fuel requires smith or mixed engine"
            );
        }
        Ok(())
    }

    pub(crate) fn smith(&self) -> wasm_smith::Config {
        let f = &self.features;
        let s = &self.sections;
        let l = &self.limits;
        let has_types = s.types && l.types > 0;
        let globals = s.globals && f.contains(F::MutableGlobals);
        let mut c = wasm_smith::Config {
            allow_invalid_funcs: false,
            allow_global_mixing: false,
            allow_floats: f.contains(F::Floats),
            gc_types_enabled: f.contains(F::GcTypes),
            function_references_enabled: f.contains(F::FunctionReferences),
            allow_start_export: s.start,
            allowed_instructions: self.instructions.smith(),
            bulk_memory_enabled: f.contains(F::BulkMemory) && s.data_count,
            bulk_memory_opt_enabled: f.contains(F::BulkMemoryOpt),
            exceptions_enabled: f.contains(F::Exceptions),
            gc_enabled: f.contains(F::Gc) && f.contains(F::GcTypes),
            custom_descriptors_enabled: f.contains(F::CustomDescriptors),
            shared_everything_threads_enabled: f.contains(F::SharedEverythingThreads),
            custom_page_sizes_enabled: f.contains(F::CustomPageSizes),
            generate_custom_sections: s.custom,
            multi_value_enabled: f.contains(F::MultiValue),
            reference_types_enabled: f.contains(F::ReferenceTypes),
            relaxed_simd_enabled: f.contains(F::RelaxedSimd),
            simd_enabled: f.contains(F::Simd),
            saturating_float_to_int_enabled: f.contains(F::SaturatingFloatToInt),
            sign_extension_ops_enabled: f.contains(F::SignExtension),
            tail_call_enabled: f.contains(F::TailCall),
            threads_enabled: f.contains(F::Threads),
            memory64_enabled: f.contains(F::Memory64),
            wide_arithmetic_enabled: f.contains(F::WideArithmetic),
            extended_const_enabled: f.contains(F::ExtendedConst),
            max_types: if has_types { l.types } else { 0 },
            max_funcs: if s.functions && has_types {
                l.functions
            } else {
                0
            },
            max_globals: if globals {
                l.globals.saturating_sub(usize::from(self.fuel.is_some()))
            } else {
                0
            },
            max_tables: if s.tables {
                l.tables.min(if f.contains(F::ReferenceTypes) {
                    l.tables
                } else {
                    1
                })
            } else {
                0
            },
            max_memories: if s.memories {
                l.memories.min(if f.contains(F::MultiMemory) {
                    l.memories
                } else {
                    1
                })
            } else {
                0
            },
            max_tags: if s.tags && has_types && f.contains(F::Exceptions) {
                l.tags
            } else {
                0
            },
            max_imports: if s.imports { l.imports } else { 0 },
            max_exports: if s.exports { l.exports } else { 0 },
            max_data_segments: if s.data { l.segments } else { 0 },
            max_element_segments: if s.elements { l.segments } else { 0 },
            max_elements: l.elements,
            max_instructions: l.instructions,
            max_memory32_bytes: l.memory_bytes.min(1_u64 << 32),
            max_memory64_bytes: l.memory_bytes as u128,
            max_table_elements: l.table_elements,
            max_type_size: 64,
            canonicalize_nans: self.canonicalize_nans,
            ..Default::default()
        };
        // wasm-smith lacks a mutable-global gate. Keep its imports/exports
        // exact for MVP by omitting globals; the structured engine covers MVP globals.
        if f.contains(F::Memtag) {
            c.min_memories = 1;
            c.min_funcs = 1;
            c.min_types = 1;
        }
        c
    }
}
