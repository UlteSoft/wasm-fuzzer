use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use wasm_fuzzer::{
    Config, Feature, Features, Profile, config::Engine, generate, inspect, validate,
};

#[derive(Parser)]
#[command(
    version,
    about = "Reproducible, valid-only Core Wasm and uwvm memtag fuzzer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Selection {
    /// Base standard. The default is Core 3.0, without experimental extensions.
    #[arg(long, value_enum)]
    profile: Option<Profile>,
    /// Enable proposals and their prerequisites; repeat or separate with commas.
    #[arg(long, value_delimiter = ',')]
    enable: Vec<Feature>,
    /// Disable proposals and their dependents, after applying enables.
    #[arg(long, value_delimiter = ',')]
    disable: Vec<Feature>,
    /// Strict JSON configuration; unknown fields are errors.
    #[arg(long)]
    config: Option<PathBuf>,
}
impl Selection {
    fn config(&self) -> Result<Config> {
        let mut config = if let Some(p) = &self.config {
            serde_json::from_slice(&read(p)?)?
        } else {
            Config::default()
        };
        if let Some(p) = self.profile {
            config.features = Features::profile(p);
        }
        for f in &self.enable {
            config.features.enable(*f);
        }
        for f in &self.disable {
            config.features.disable(*f);
        }
        config.check()?;
        Ok(config)
    }
}

#[derive(Subcommand)]
enum Command {
    /// List the complete supported feature/extension inventory.
    Features,
    /// Generate exhaustive or interaction-complete feature-combination corpora.
    Matrix {
        #[arg(long, value_enum, default_value = "official")]
        scope: wasm_fuzzer::matrix::Scope,
        #[arg(long, value_enum, default_value = "pairwise")]
        strength: wasm_fuzzer::matrix::Strength,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value_t = 2)]
        count: u64,
        #[arg(long, value_enum, default_value = "mixed")]
        engine: Engine,
        #[arg(long)]
        imports: bool,
        #[arg(long)]
        plan_only: bool,
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// Print the effective configuration, suitable for --config.
    Config {
        #[command(flatten)]
        selection: Selection,
    },
    /// Generate a deterministic corpus plus per-file replay metadata.
    Generate {
        #[command(flatten)]
        selection: Selection,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value_t = 1)]
        count: u64,
        /// Destination must not already exist. Nothing is overwritten.
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long, value_enum)]
        engine: Option<Engine>,
        /// Permit arbitrary Core imports (never adds a WASI harness).
        #[arg(long)]
        imports: bool,
        /// Disable start sections, useful for consumers that instantiate modules.
        #[arg(long)]
        no_start: bool,
    },
    /// Validate exact input bytes against the selected feature set, including memtag metadata.
    Validate {
        #[command(flatten)]
        selection: Selection,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Report sections, instructions, memories and proposal structures as JSON.
    Inspect { file: PathBuf },
    /// Regenerate a saved case and compare every byte and the SHA-256.
    Replay {
        metadata: PathBuf,
        #[arg(long)]
        wasm: Option<PathBuf>,
    },
    /// Produce a typed standard-Wasm projection for auxiliary official validation.
    ProjectMemtag {
        #[command(flatten)]
        selection: Selection,
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn read(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        fs::metadata(path)?.len() <= 64 * 1024 * 1024,
        "input exceeds 64 MiB: {}",
        path.display()
    );
    fs::read(path).with_context(|| format!("read {}", path.display()))
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(bytes)?;
    Ok(())
}
fn corpus(config: &Config, seed: u64, count: u64, out_dir: &Path) -> Result<()> {
    ensure!(count > 0 && count <= 1_000_000, "count must be 1..=1000000");
    ensure!(
        seed.checked_add(count - 1).is_some(),
        "seed range overflows u64"
    );
    config.check()?;
    fs::create_dir(out_dir).context("out-dir must be new and its parent must exist")?;
    create(
        &out_dir.join("config.json"),
        &serde_json::to_vec_pretty(config)?,
    )?;
    let mut manifest = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out_dir.join("manifest.jsonl"))?;
    for n in 0..count {
        let generated = generate(config, seed + n)?;
        let base = format!("{:016x}", seed + n);
        create(&out_dir.join(format!("{base}.wasm")), &generated.bytes)?;
        create(
            &out_dir.join(format!("{base}.json")),
            &serde_json::to_vec_pretty(&generated.metadata)?,
        )?;
        writeln!(
            manifest,
            "{}",
            serde_json::json!({"seed":seed+n,"file":format!("{base}.wasm"),"sha256":generated.metadata.sha256,"attempt":generated.metadata.attempt,"engine":generated.metadata.engine})
        )?;
    }
    Ok(())
}
fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Features => {
            for feature in Feature::ALL {
                println!(
                    "{}\tdependencies: {}",
                    feature.name(),
                    feature
                        .dependencies()
                        .iter()
                        .map(|f| f.name())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            println!("memory64 includes table64; memtag uses FF packed v1/linear1 and globals v2");
        }
        Command::Matrix {
            scope,
            strength,
            seed,
            count,
            engine,
            imports,
            plan_only,
            out_dir,
        } => {
            use wasm_fuzzer::matrix;
            let plan = matrix::plan(scope, strength)?;
            ensure!(
                count > 0 && seed.checked_add(count - 1).is_some(),
                "invalid seed range"
            );
            fs::create_dir(&out_dir).context("matrix out-dir must be new")?;
            create(
                &out_dir.join("matrix.json"),
                &serde_json::to_vec_pretty(&serde_json::json!({
                    "format": 1, "generator_version": env!("CARGO_PKG_VERSION"), "scope":scope,
                    "strength":strength, "dimensions":scope.dimensions(), "configurations":plan.len(),
                    "seed":seed, "count":count, "engine":engine, "plan_only":plan_only,
                    "corpora":plan.iter().map(|f| format!("{:08x}", matrix::mask(f))).collect::<Vec<_>>(),
                    "features":plan,
                }))?,
            )?;
            if !plan_only {
                for (n, features) in plan.iter().enumerate() {
                    let mut config = Config {
                        features: features.clone(),
                        engine,
                        ..Config::default()
                    };
                    config.sections.imports = imports;
                    corpus(
                        &config,
                        seed,
                        count,
                        &out_dir.join(format!("{:08x}", matrix::mask(features))),
                    )?;
                    if (n + 1) % 256 == 0 {
                        eprintln!("validated {} / {} configurations", n + 1, plan.len());
                    }
                }
            }
            println!(
                "{} feature configurations: {}",
                plan.len(),
                out_dir.display()
            );
        }
        Command::Config { selection } => {
            println!("{}", serde_json::to_string_pretty(&selection.config()?)?)
        }
        Command::Generate {
            selection,
            seed,
            count,
            out_dir,
            engine,
            imports,
            no_start,
        } => {
            ensure!(count > 0 && count <= 1_000_000, "count must be 1..=1000000");
            ensure!(
                seed.checked_add(count - 1).is_some(),
                "seed range overflows u64"
            );
            let mut config = selection.config()?;
            if let Some(engine) = engine {
                config.engine = engine;
            }
            if imports {
                config.sections.imports = true;
            }
            if no_start {
                config.sections.start = false;
            }
            corpus(&config, seed, count, &out_dir)?;
            println!(
                "generated and validated {count} modules in {}",
                out_dir.display()
            );
        }
        Command::Validate { selection, files } => {
            let config = selection.config()?;
            for path in files {
                validate(&read(&path)?, &config.features)
                    .with_context(|| format!("{}", path.display()))?;
                println!("valid: {}", path.display());
            }
        }
        Command::Inspect { file } => println!(
            "{}",
            serde_json::to_string_pretty(&inspect(&read(&file)?)?)?
        ),
        Command::Replay { metadata, wasm } => {
            let meta: wasm_fuzzer::generate::Metadata = serde_json::from_slice(&read(&metadata)?)?;
            ensure!(
                meta.format == 1 && meta.generator_version == env!("CARGO_PKG_VERSION"),
                "incompatible replay metadata version"
            );
            let expected = read(&wasm.unwrap_or_else(|| metadata.with_extension("wasm")))?;
            let regenerated = generate(&meta.config, meta.seed)?;
            ensure!(regenerated.bytes == expected, "replay byte mismatch");
            ensure!(
                regenerated.metadata.sha256 == meta.sha256
                    && regenerated.metadata.attempt == meta.attempt
                    && regenerated.metadata.bytes == meta.bytes,
                "replay metadata mismatch"
            );
            println!("100% byte replay: {}", meta.sha256);
        }
        Command::ProjectMemtag {
            selection,
            input,
            output,
        } => {
            let config = selection.config()?;
            let bytes = wasm_fuzzer::memtag::project(&read(&input)?, &config.features)?;
            create(&output, &bytes)?;
            println!(
                "wrote typed projection (not executable-equivalent): {}",
                output.display()
            );
        }
    }
    Ok(())
}
