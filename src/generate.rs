use crate::{Config, Feature, Inventory, config::Engine, inspect, memtag, validate, witnesses};
use anyhow::{Context, Result, bail, ensure};
use arbitrary::Unstructured;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SplitMix64, specified here so that seeds never depend on rand crate versions.
pub(crate) struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub format: u32,
    pub generator_version: String,
    pub seed: u64,
    pub attempt: u32,
    pub engine: Engine,
    pub sha256: String,
    pub bytes: usize,
    pub config: Config,
    pub inventory: Inventory,
}
#[derive(Debug)]
pub struct Generated {
    pub bytes: Vec<u8>,
    pub metadata: Metadata,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn finish(config: &Config, bytes: Vec<u8>, rng: &mut Rng) -> Result<Vec<u8>> {
    let bytes = crate::encoding::augment(&bytes, config, rng)?;
    // Validate the base as well: augmentation must not conceal generator failures.
    validate(&bytes, &config.features).context("base module validation")?;
    let bytes = if config.features.contains(Feature::Memtag) {
        memtag::inject(&bytes, config, rng)?
    } else {
        bytes
    };
    ensure!(
        bytes.len() <= config.limits.module_bytes,
        "generated module exceeds module_bytes"
    );
    validate(&bytes, &config.features).context("final exact-feature validation")?;
    let inv = inspect(&bytes)?;
    for (enabled, ids, name) in [
        (config.sections.types, &[1][..], "types"),
        (config.sections.imports, &[2], "imports"),
        (config.sections.functions, &[3, 10], "functions"),
        (config.sections.tables, &[4], "tables"),
        (config.sections.memories, &[5], "memories"),
        (config.sections.globals, &[6], "globals"),
        (config.sections.exports, &[7], "exports"),
        (config.sections.start, &[8], "start"),
        (config.sections.elements, &[9], "elements"),
        (config.sections.data, &[11], "data"),
        (config.sections.data_count, &[12], "data-count"),
        (config.sections.tags, &[13], "tags"),
        (config.sections.custom, &[0], "custom"),
    ] {
        ensure!(
            enabled || ids.iter().all(|id| !inv.sections.contains_key(id)),
            "disabled {name} section generated"
        );
    }
    Ok(bytes)
}

/// Coverage-guided fuzzers can pass their byte stream directly to wasm-smith.
/// Insufficient entropy or a rejected candidate is an error, never invalid Wasm.
pub fn generate_from_bytes(config: &Config, input: &[u8]) -> Result<Vec<u8>> {
    config.check()?;
    let mut module = wasm_smith::Module::new(config.smith(), &mut Unstructured::new(input))?;
    if let Some(fuel) = config.fuel {
        module.ensure_termination(fuel)?;
    }
    let hash = Sha256::digest(input);
    let mut rng = Rng(u64::from_le_bytes(hash[..8].try_into().unwrap()));
    finish(config, module.to_bytes(), &mut rng)
}

/// Deterministic bounded retries. Failure is explicit; no empty-module fallback.
pub fn generate(config: &Config, seed: u64) -> Result<Generated> {
    config.check()?;
    let mut last_error = String::new();
    for attempt in 0..config.limits.attempts {
        let mut rng = Rng(seed ^ (attempt as u64).wrapping_mul(0xd6e8feb86659fd93));
        let engine = match config.engine {
            Engine::Mixed if config.fuel.is_none() && seed.is_multiple_of(4) => Engine::Structured,
            Engine::Mixed => Engine::Smith,
            e => e,
        };
        let candidate = match engine {
            Engine::Structured => {
                witnesses::module(config, &mut rng).and_then(|b| finish(config, b, &mut rng))
            }
            Engine::Smith => {
                let mut entropy = Vec::with_capacity(config.limits.entropy_bytes + 8);
                while entropy.len() < config.limits.entropy_bytes {
                    entropy.extend_from_slice(&rng.next().to_le_bytes());
                }
                entropy.truncate(config.limits.entropy_bytes);
                generate_from_bytes(config, &entropy)
            }
            Engine::Mixed => unreachable!(),
        };
        match candidate {
            Ok(bytes) => {
                let metadata = Metadata {
                    format: 1,
                    generator_version: env!("CARGO_PKG_VERSION").into(),
                    seed,
                    attempt,
                    engine,
                    sha256: sha256(&bytes),
                    bytes: bytes.len(),
                    config: config.clone(),
                    inventory: inspect(&bytes)?,
                };
                return Ok(Generated { bytes, metadata });
            }
            Err(error) => last_error = format!("{error:#}"),
        }
    }
    bail!(
        "seed {seed}: all {} attempts rejected; last error: {last_error}",
        config.limits.attempts
    )
}
