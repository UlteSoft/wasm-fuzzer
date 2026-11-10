//! Type-driven generation of valid Core Wasm, with exact proposal gates.
//! All public generation paths validate before returning bytes.
pub mod config;
pub mod features;
pub mod generate;
pub mod inspect;
pub mod memtag;
pub mod witnesses;

pub use config::Config;
pub use features::{Feature, Features, Profile};
pub use generate::{Generated, generate, generate_from_bytes};
pub use inspect::{Inventory, inspect, validate};
