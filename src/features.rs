use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, str::FromStr};
use wasmparser_memtag::WasmFeatures as W;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    #[value(alias = "wasmmvp", alias = "mvp")]
    WasmMvp,
    #[value(alias = "wasm1.0", alias = "1.0")]
    Wasm1,
    #[value(alias = "wasm2.0", alias = "2.0")]
    Wasm2,
    #[value(alias = "wasm3.0", alias = "3.0")]
    Wasm3,
    All,
}

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    MutableGlobals,
    SaturatingFloatToInt,
    SignExtension,
    MultiValue,
    BulkMemory,
    ReferenceTypes,
    Simd,
    RelaxedSimd,
    TailCall,
    ExtendedConst,
    FunctionReferences,
    Gc,
    MultiMemory,
    Memory64,
    Exceptions,
    Threads,
    WideArithmetic,
    CustomPageSizes,
    Memtag,
}

impl Feature {
    pub const ALL: [Self; 19] = [
        Self::MutableGlobals,
        Self::SaturatingFloatToInt,
        Self::SignExtension,
        Self::MultiValue,
        Self::BulkMemory,
        Self::ReferenceTypes,
        Self::Simd,
        Self::RelaxedSimd,
        Self::TailCall,
        Self::ExtendedConst,
        Self::FunctionReferences,
        Self::Gc,
        Self::MultiMemory,
        Self::Memory64,
        Self::Exceptions,
        Self::Threads,
        Self::WideArithmetic,
        Self::CustomPageSizes,
        Self::Memtag,
    ];

    pub fn name(self) -> &'static str {
        use Feature::*;
        match self {
            MutableGlobals => "mutable-globals",
            SaturatingFloatToInt => "saturating-float-to-int",
            SignExtension => "sign-extension",
            MultiValue => "multi-value",
            BulkMemory => "bulk-memory",
            ReferenceTypes => "reference-types",
            Simd => "simd",
            RelaxedSimd => "relaxed-simd",
            TailCall => "tail-call",
            ExtendedConst => "extended-const",
            FunctionReferences => "function-references",
            Gc => "gc",
            MultiMemory => "multi-memory",
            Memory64 => "memory64",
            Exceptions => "exceptions",
            Threads => "threads",
            WideArithmetic => "wide-arithmetic",
            CustomPageSizes => "custom-page-sizes",
            Memtag => "memtag",
        }
    }

    pub fn dependencies(self) -> &'static [Self] {
        use Feature::*;
        match self {
            RelaxedSimd => &[Simd],
            FunctionReferences => &[ReferenceTypes],
            Gc => &[FunctionReferences, ReferenceTypes],
            _ => &[],
        }
    }

    fn flag(self) -> W {
        use Feature::*;
        match self {
            MutableGlobals => W::MUTABLE_GLOBAL,
            SaturatingFloatToInt => W::SATURATING_FLOAT_TO_INT,
            SignExtension => W::SIGN_EXTENSION,
            MultiValue => W::MULTI_VALUE,
            BulkMemory => W::BULK_MEMORY,
            ReferenceTypes => W::REFERENCE_TYPES,
            Simd => W::SIMD,
            RelaxedSimd => W::RELAXED_SIMD,
            TailCall => W::TAIL_CALL,
            ExtendedConst => W::EXTENDED_CONST,
            FunctionReferences => W::FUNCTION_REFERENCES,
            Gc => W::GC,
            MultiMemory => W::MULTI_MEMORY,
            Memory64 => W::MEMORY64,
            Exceptions => W::EXCEPTIONS,
            Threads => W::THREADS,
            WideArithmetic => W::WIDE_ARITHMETIC,
            CustomPageSizes => W::CUSTOM_PAGE_SIZES,
            Memtag => W::MEMTAG_EXTENSION,
        }
    }
}

impl FromStr for Feature {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == value)
            .ok_or_else(|| anyhow::anyhow!("unknown feature {value:?}"))
    }
}

/// An exact, serializable proposal set. No validator defaults are inherited.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Features(pub BTreeSet<Feature>);

impl Default for Features {
    fn default() -> Self {
        Self::profile(Profile::Wasm3)
    }
}

impl Features {
    pub fn profile(profile: Profile) -> Self {
        use Feature::*;
        let mut set = Self(BTreeSet::new());
        if profile == Profile::WasmMvp {
            return set;
        }
        set.enable(MutableGlobals);
        if profile == Profile::Wasm1 {
            return set;
        }
        for f in [
            SaturatingFloatToInt,
            SignExtension,
            MultiValue,
            BulkMemory,
            ReferenceTypes,
            Simd,
        ] {
            set.enable(f);
        }
        if profile == Profile::Wasm2 {
            return set;
        }
        for f in [
            RelaxedSimd,
            TailCall,
            ExtendedConst,
            FunctionReferences,
            Gc,
            MultiMemory,
            Memory64,
            Exceptions,
        ] {
            set.enable(f);
        }
        // Threads, wide arithmetic, custom pages and memtag are independent
        // proposals, not silently included in the official Core 3 preset.
        if profile == Profile::All {
            for f in Feature::ALL {
                set.enable(f);
            }
        }
        set
    }
    pub fn contains(&self, f: Feature) -> bool {
        self.0.contains(&f)
    }
    pub fn enable(&mut self, f: Feature) {
        for &dependency in f.dependencies() {
            self.enable(dependency);
        }
        self.0.insert(f);
    }
    /// Disabling a prerequisite also disables its dependents, to keep the set coherent.
    pub fn disable(&mut self, f: Feature) {
        self.0.remove(&f);
        for dependent in Feature::ALL {
            if dependent.dependencies().contains(&f) {
                self.disable(dependent);
            }
        }
    }
    pub fn check(&self) -> Result<()> {
        for f in &self.0 {
            for d in f.dependencies() {
                if !self.contains(*d) {
                    bail!("{} requires {}", f.name(), d.name());
                }
            }
        }
        Ok(())
    }
    pub fn validator(&self) -> W {
        self.0.iter().fold(W::MVP, |flags, f| flags | f.flag())
    }
    pub fn standard_validator(&self) -> wasmparser::WasmFeatures {
        wasmparser::WasmFeatures::from_bits_truncate(self.validator().bits())
    }
}
