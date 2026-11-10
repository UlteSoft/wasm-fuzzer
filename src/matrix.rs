//! Deterministic exhaustive and interaction-complete feature configuration plans.
use crate::{Feature, Features, Profile};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    Official,
    Core,
    All,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Strength {
    Exhaustive,
    Pairwise,
    Triplewise,
}

impl Scope {
    pub fn dimensions(self) -> Vec<Feature> {
        match self {
            Self::Official => Feature::OFFICIAL_PROPOSALS.to_vec(),
            Self::Core => Feature::ALL.into_iter().filter(|f| f.official()).collect(),
            Self::All => Feature::ALL.to_vec(),
        }
    }
    fn baseline(self) -> Features {
        Features::profile(if matches!(self, Self::Official) {
            Profile::WasmMvp
        } else {
            Profile::Empty
        })
    }
}

pub fn mask(features: &Features) -> u64 {
    features.0.iter().fold(0, |mask, &f| mask | (1 << f as u32))
}

/// Include every feasible on/off assignment to each tuple. Both sparse and
/// dense contexts are retained; dependencies are never silently violated.
pub fn plan(scope: Scope, strength: Strength) -> Result<Vec<Features>> {
    let dimensions = scope.dimensions();
    let baseline = scope.baseline();
    let mut plans = BTreeMap::new();
    if matches!(strength, Strength::Exhaustive) {
        ensure!(
            dimensions.len() <= 20,
            "exhaustive plans are limited to 20 dimensions; use triplewise for all proposals"
        );
        for bits in 0..1_u64 << dimensions.len() {
            let mut features = baseline.clone();
            for (bit, &f) in dimensions.iter().enumerate() {
                if bits & (1 << bit) != 0 {
                    features.enable(f);
                }
            }
            plans.insert(mask(&features), features);
        }
    } else {
        let n = if matches!(strength, Strength::Pairwise) {
            2
        } else {
            3
        };
        let mut full = baseline.clone();
        for &f in &dimensions {
            full.enable(f);
        }
        fn visit(
            dimensions: &[Feature],
            indices: &mut Vec<usize>,
            n: usize,
            start: usize,
            baseline: &Features,
            full: &Features,
            plans: &mut BTreeMap<u64, Features>,
        ) {
            if indices.len() == n {
                for bits in 0..1 << n {
                    for initial in [baseline, full] {
                        let mut f = initial.clone();
                        for (bit, &idx) in indices.iter().enumerate() {
                            if bits & (1 << bit) != 0 {
                                f.enable(dimensions[idx]);
                            }
                        }
                        for (bit, &idx) in indices.iter().enumerate() {
                            if bits & (1 << bit) == 0 {
                                f.disable(dimensions[idx]);
                            }
                        }
                        if indices.iter().enumerate().all(|(bit, &idx)| {
                            f.contains(dimensions[idx]) == (bits & (1 << bit) != 0)
                        }) {
                            plans.insert(mask(&f), f);
                        }
                    }
                }
                return;
            }
            for idx in start..dimensions.len() {
                indices.push(idx);
                visit(dimensions, indices, n, idx + 1, baseline, full, plans);
                indices.pop();
            }
        }
        visit(
            &dimensions,
            &mut Vec::new(),
            n,
            0,
            &baseline,
            &full,
            &mut plans,
        );
    }
    for features in plans.values() {
        features.check()?;
    }
    Ok(plans.into_values().collect())
}
