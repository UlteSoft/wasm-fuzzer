use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, str::FromStr};
use wasmparser_memtag::WasmFeatures as W;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    #[value(alias = "none")]
    Empty,
    #[value(alias = "wasmmvp", alias = "mvp")]
    WasmMvp,
    #[value(alias = "wasm1.0", alias = "1.0")]
    Wasm1,
    #[value(alias = "wasm2.0", alias = "2.0")]
    Wasm2,
    #[value(alias = "wasm3.0", alias = "3.0")]
    Wasm3,
    Lime1,
    #[value(alias = "all-known")]
    AllCore,
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
    Floats,
    GcTypes,
    BulkMemoryOpt,
    CallIndirectOverlong,
    LegacyExceptions,
    SharedEverythingThreads,
    MemoryControl,
    StackSwitching,
    CustomDescriptors,
    CompactImports,
}

impl Feature {
    pub const ALL: [Self; 29] = [
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
        Self::Floats,
        Self::GcTypes,
        Self::BulkMemoryOpt,
        Self::CallIndirectOverlong,
        Self::LegacyExceptions,
        Self::SharedEverythingThreads,
        Self::MemoryControl,
        Self::StackSwitching,
        Self::CustomDescriptors,
        Self::CompactImports,
    ];

    /// The independent proposals included in the official Core 3 release.
    pub const OFFICIAL_PROPOSALS: [Self; 15] = [
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
    ];

    pub fn official(self) -> bool {
        Self::OFFICIAL_PROPOSALS.contains(&self)
            || matches!(
                self,
                Self::Floats | Self::GcTypes | Self::BulkMemoryOpt | Self::CallIndirectOverlong
            )
    }

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
            Floats => "floats",
            GcTypes => "gc-types",
            BulkMemoryOpt => "bulk-memory-opt",
            CallIndirectOverlong => "call-indirect-overlong",
            LegacyExceptions => "legacy-exceptions",
            SharedEverythingThreads => "shared-everything-threads",
            MemoryControl => "memory-control",
            StackSwitching => "stack-switching",
            CustomDescriptors => "custom-descriptors",
            CompactImports => "compact-imports",
        }
    }

    pub fn dependencies(self) -> &'static [Self] {
        use Feature::*;
        match self {
            BulkMemory => &[BulkMemoryOpt],
            ReferenceTypes => &[CallIndirectOverlong],
            RelaxedSimd => &[Simd],
            FunctionReferences => &[ReferenceTypes],
            Gc => &[FunctionReferences],
            LegacyExceptions => &[Exceptions],
            StackSwitching => &[FunctionReferences],
            CustomDescriptors => &[Gc],
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
            Floats => W::FLOATS,
            GcTypes => W::GC_TYPES,
            BulkMemoryOpt => W::BULK_MEMORY_OPT,
            CallIndirectOverlong => W::CALL_INDIRECT_OVERLONG,
            LegacyExceptions => W::LEGACY_EXCEPTIONS,
            SharedEverythingThreads => W::SHARED_EVERYTHING_THREADS,
            MemoryControl => W::MEMORY_CONTROL,
            StackSwitching => W::STACK_SWITCHING,
            CustomDescriptors => W::CUSTOM_DESCRIPTORS,
            CompactImports => W::COMPACT_IMPORTS,
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
        if profile == Profile::Empty {
            return set;
        }
        set.enable(Floats);
        set.enable(GcTypes);
        if profile == Profile::WasmMvp {
            return set;
        }
        set.enable(MutableGlobals);
        if profile == Profile::Wasm1 {
            return set;
        }
        if profile == Profile::Lime1 {
            for f in [
                MultiValue,
                SignExtension,
                SaturatingFloatToInt,
                BulkMemoryOpt,
                ExtendedConst,
                CallIndirectOverlong,
            ] {
                set.enable(f);
            }
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
        if matches!(profile, Profile::All | Profile::AllCore) {
            for f in Feature::ALL {
                if f != Memtag || profile == Profile::All {
                    set.enable(f);
                }
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
        self.0.iter().fold(W::empty(), |flags, f| flags | f.flag())
    }
    pub fn standard_validator(&self) -> wasmparser::WasmFeatures {
        wasmparser::WasmFeatures::from_bits_truncate(self.validator().bits())
    }
}
