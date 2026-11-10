# wasm-fuzzer

A Rust generator for **valid, feature-selected WebAssembly Core modules**, from
the MVP through Core 3.0, with an independent implementation of the uwvm linear
memory-tagging extension. It produces ordinary `.wasm` binaries. The default
configuration has no imports and requires no WASI, JavaScript, or component host.

Every successful generation passes an exact-feature validator before its bytes
are returned or written. Standard modules additionally pass the unmodified
upstream `wasmparser` validator. Rejected candidates have a bounded retry budget;
exhaustion is an error, never an empty-module substitution or an invalid output.

## Build

Rust 1.85 or later is required; the qualification toolchain is Rust 1.93.1.
Dependencies are pinned by `Cargo.lock`. Both local vendor patches are documented
in [THIRD_PARTY.md](THIRD_PARTY.md).

```sh
mkdir -p .tmp
cd .tmp
CARGO_HOME="$PWD/cargo" TMPDIR="$PWD" \
  cargo build --locked --release --manifest-path ../Cargo.toml
./target/release/wasm-fuzzer features
```

For resource-constrained builds and **all repository tests**, use the existing
64 GiB, zero-swap cgroup as described below. Builds use `.tmp/target`; `.tmp`,
generated Wasm, logs, corpora, reports, downloaded tools, and caches are excluded
from Git. No toolchain installation or external validator is required to use the
Rust library or generation CLI once the Cargo dependencies are available.

## Generate and replay

Run these examples from `.tmp` (or pass them through the cgroup runner).
Each output directory must be new. Existing files are never overwritten.

```sh
target/release/wasm-fuzzer generate --profile wasmmvp --seed 0 --count 100 --out-dir mvp
target/release/wasm-fuzzer generate --profile wasm2.0 --enable gc --seed 100 --count 100 --out-dir gc
target/release/wasm-fuzzer generate --profile wasm3.0 --enable memtag --seed 200 --count 100 --out-dir tagged
target/release/wasm-fuzzer generate --profile wasm3 --disable gc,memory64 --out-dir reduced

target/release/wasm-fuzzer validate --config tagged/config.json tagged/00000000000000c8.wasm
target/release/wasm-fuzzer replay tagged/00000000000000c8.json
target/release/wasm-fuzzer inspect tagged/00000000000000c8.wasm
```

Each corpus contains the effective `config.json`, a streaming `manifest.jsonl`,
and a `.wasm` / `.json` pair per seed. Metadata records the engine, attempt,
configuration, SHA-256, byte size, and structural inventory. Replay regenerates
the module and compares every byte, the digest, size, and attempt number.
Keep the source revision and `Cargo.lock` with a corpus for long-term replay.

Generation is deterministic, using a specified SplitMix64 entropy expansion.
Seeds include zero and the full `u64` range. CLI seed ranges reject overflow.
`--imports` permits arbitrary Core imports; `--no-start` suppresses start sections.

## Features

| Preset | Allowed features |
| --- | --- |
| `wasmmvp` / `wasm-mvp` | Original Core instructions and types; no imported/exported mutable globals |
| `wasm1` / `wasm1.0` | MVP plus mutable-global imports/exports |
| `wasm2` / `wasm2.0` | Core 1 plus sign extension, saturating conversion, multi-value, bulk memory, reference types and SIMD |
| `wasm3` / `wasm3.0` | Core 2 plus GC, typed function references, tail calls, extended constant expressions, multiple memories, memory64/table64, exceptions and relaxed SIMD |
| `all` | Core 3 plus threads, wide arithmetic, custom page sizes and memtag |

Every row can be modified with repeatable, comma-separated `--enable` and
`--disable` options. All 19 switches are listed by `features`. Enabled features
are permissions, not a guarantee that every random module uses each feature.

Enabling `gc` also enables `function-references` and `reference-types`.
Enabling `relaxed-simd` enables `simd`. Disabling a prerequisite disables its
dependents. CLI ordering is deterministic: configuration, optional profile,
all enables, then all disables. A JSON feature set is exact and missing
prerequisites are rejected. Unknown fields, proposals, and reserved memtag
operations are errors.

`memory64` covers both 64-bit memories and tables, as in the proposal. Threads,
wide arithmetic, custom page sizes and memtag remain separate from the Core 3
preset. Legacy exceptions, stack switching, shared-everything threads, custom
descriptors, and the component model are outside this project's Core 3 scope.

## Generation engines and controls

`--engine mixed` is the default: one seed in four uses the structured engine,
and the others use `wasm-smith`. `--engine smith` selects the general type-driven
generator. It covers recursive and subtype groups, typed operands, structured
control, scalar and vector instructions, references, GC, tables, memories,
segments, tags, and exceptions. `--engine structured` supplies randomized
coverage cases, including function references without GC.

```sh
target/release/wasm-fuzzer config --profile wasm2 --enable gc > config.json
# Edit config.json, then:
target/release/wasm-fuzzer generate --config config.json --seed 42 --count 100 --out-dir configured
```

The JSON configuration provides:

- Section switches: types, imports, functions/code, tables, memories, tags,
  globals, exports, start, elements, data, data-count, and custom sections.
  A final inventory check rejects any disabled section. Dependent content is
  suppressed when its supporting section is unavailable.
- Instruction selection categories: numeric, vector, reference, parametric,
  variable, table, memory, control, and aggregate. Typed constants, drops,
  function endings and other required scaffolding can still appear; use feature
  switches to forbid a proposal completely.
- Generation budgets for types, functions, globals, tables, memories, tags,
  imports, exports, segments, elements, instructions, and entropy. These guide
  construction; type groups and required scaffolding can add overhead. The
  final `module_bytes` limit and retry limit are enforced before return.
- Memory/table size bounds, NaN canonicalization, optional loop/recursion fuel,
  memtag sequence count, an operation whitelist, and globals metadata generation.

The optional `fuel` setting selects the smith path in mixed mode and instruments
loops and calls. It requires globals and is rejected with the structured engine.
It does not make arbitrary imports terminate or guarantee trap-free execution.
Validation does not imply successful instantiation or execution: division by
zero, out-of-bounds accesses, exception throws, or tag faults are useful valid
test cases. This project does not execute generated guest code during acceptance.

## memtag wire and validation

The extension is independent of the standard presets. All 19 executable
operations are supported, including status/geometry, extract/insert/copy/sub,
load/untag, store and zeroing forms, random forms, and hint forms. The reserved
`memtag.add` (`0x33`) is never generated and is rejected.

```text
FF ULEB32((1 << 24) | (1 << 16) | operation) ULEB32(memory_index)
```

Every operand and result uses the selected memory's address type, including tag
values and lengths. Imported memories, nonzero memory indices, mixed memory32 /
memory64, and shared memories are supported. Random sequences are inserted at
instruction boundaries with zero net stack effect, including within nested
control flow. With the default 24 sequences per function, every enabled
operation is covered in each generated function.

`uwvm.memtag.linear.globals` uses version 2: a ULEB64 version/count followed by
ULEB32 memory index, ULEB64 address, and ULEB64 length triples. Validation checks
framing, overflow, memory indices, minimum memory bounds, and overlapping
16-byte allocation granules. Duplicate sections and historical `memtag.globals`
metadata are rejected. Cross-module imported-memory alias ownership requires a
linker/runtime and is not established by static validation of one module.

The contract follows the local `uwvm2-ros` implementation and the reader in
`wasm-git`; see [THIRD_PARTY.md](THIRD_PARTY.md). It is not an official Core opcode.

## Three independent acceptance gates

With OCaml, dune and menhir available on `PATH`, the pinned official reference
can be built entirely inside scratch storage:

```sh
tools/run-in-cgroup.sh bash ../tools/build-reference.sh
# Result: .tmp/reference/wasm

# Optional, for modules with imports; requires a C++26 compiler.
CXX=clang++-21 tools/run-in-cgroup.sh bash ../tools/build-uwvm-validator.sh /absolute/path/to/uwvm2-ros
# Result: .tmp/uwvm-validator/validate
```

`tools/verify.py` runs the requested validation and replay pipeline:

1. **Official WebAssembly reference validator:** the executable built from
   [WebAssembly/spec](https://github.com/WebAssembly/spec/tree/main/interpreter),
   invoked with `-d` so it validates without instantiating or executing modules.
2. **uwvm2-ros:** validation mode, with individual feature enable/disable flags
   translated from the corpus configuration. No version preset is combined with
   individual flags, since uwvm rejects that combination.
3. **wasm-git:** original modules must validate; forward and reverse audit edits
   must reconstruct every byte, including unchanged gaps. Serialized patches
   are also created, applied in both directions, and compared byte-for-byte.

The uwvm command-line loader requires providers for imports even in validation
mode. With `--uwvm-pure`, imported modules instead use the small test adapter
in `tools/uwvm-validator.cc`. It calls uwvm's production module parser, validates
every function with its Core 3 validator, and checks static memtag metadata with
uwvm's extension implementation. It neither links imports nor executes guests.
The build records hashes of all included source headers; the acceptance report
identifies the binary used for each case. This optional C++ integration tool is
separate from the Rust generator and library.

One feature-taxonomy difference is explicit: GC permits constant expressions to
read previously defined immutable globals; the current uwvm parser gates these
reads with its `extended-const` switch. The adapter enables that uwvm switch for
GC and records the full mapping in the report. The preceding standard validator
still enforces the original exact feature set, so arithmetic extended constants
are not admitted into a `wasm2+gc` corpus. Use `--uwvm-strict-flags` to disable
this adaptation and retain the resulting validator discrepancies as failures.

The official interpreter cannot accept private `0xff` memtag instructions.
For tagged samples the pipeline requires rejection of the original, then
validates a typed standard projection. Projection replaces each memtag operation
with input drops and typed zero results; it preserves typing, **not semantics**.
Both uwvm and wasm-git still validate and replay the original tagged bytes.
Reports distinguish these results explicitly. Extra proposals absent from the
official Core reference are rejected by this three-gate runner rather than
silently counted as passing.

```sh
# From the repository root; paths to external binaries are explicit.
tools/run-in-cgroup.sh python3 ../tools/verify.py mvp gc tagged \
  --generator target/release/wasm-fuzzer \
  --official /absolute/path/to/WebAssembly/spec/interpreter/wasm \
  --uwvm /absolute/path/to/uwvm2-ros \
  --uwvm-pure uwvm-validator/validate \
  --wasm-git /absolute/path/to/wasm-git \
  --output verification
```

The runner also checks invalid controls, per-case seed replay, SHA-256 digests,
and command timeouts. Reports include the exact external binary hashes, cgroup
identity, every case outcome, and retained stdout/stderr. A nonzero exit is a
failed acceptance run; all evidence stays below `.tmp`.

## Resource-constrained tests

All repository tests require Linux and the **existing 64 GiB / zero-swap cgroup**.
The runner verifies both limits, joins only its own process, sets temporary,
Cargo, and cache paths under `.tmp`, and executes there. It never changes limits
or terminates another workload. The default existing resource container is
`uwvm-sdk-resource-gate-20261024`; override it with
`WASM_FUZZER_RESOURCE_CONTAINER`, or set an absolute `WASM_FUZZER_CGROUP`.

If direct cgroup membership writes are unavailable, the runner uses a
short-lived Docker helper with networking disabled, a read-only root,
all capabilities dropped, and only the membership file mounted. This helper
moves the waiting runner into the existing group; the tests run natively in
that existing group, not in a replacement resource container.

```sh
# Optional: location of an already installed Rust toolchain's bin directory.
export WASM_FUZZER_TOOLCHAIN=/absolute/path/to/toolchain/bin
tools/run-in-cgroup.sh bash ../tools/check.sh

# Increase randomized feature combinations without changing the seed schedule.
WASM_FUZZER_MATRIX_CASES=5000 tools/run-in-cgroup.sh \
  cargo test --locked --manifest-path ../Cargo.toml --all-targets
```

Tests cover standard profiles, proposal-by-proposal enablement, deterministic
replay, feature swarms, every section/category switch, boundary configurations,
fuel, all memtag signatures at both address widths, imported-memory indexing,
malformed extension encodings, metadata range overlap, CLI errors, and overwrite
refusal. External three-gate tests use the separate command above.

The 2026-11-11 qualification passed all 12 integration tests with 5,000 feature
swarm cases, formatting, Clippy with warnings denied, and a release build on
Rust 1.93.1. External acceptance passed 640 cases: 384 standard originals and
256 memtag originals with official typed-projection checks. The corpus covered
MVP, Core 2, Core 2 + GC, Core 3, MVP/2/3 + memtag, and Core 3 imports with and
without memtag, including seeds through `u64::MAX`. All 640 passed forward and
reverse audit and serialized-patch replay with wasm-git. uwvm validated 559
through its CLI and 81 imported modules through its production validation API.
The existing cgroup recorded no OOM events. Machine-readable reports and all
intermediate artifacts remain in ignored `.tmp` storage.

## Library API

```rust,ignore
use wasm_fuzzer::{Config, Feature, Features, Profile, generate, validate};

let mut config = Config::default();
config.features = Features::profile(Profile::Wasm2);
config.features.enable(Feature::Gc);
config.features.enable(Feature::Memtag);
let case = generate(&config, 42)?;
validate(&case.bytes, &config.features)?;
```

`generate_from_bytes(&config, input)` accepts entropy directly for integration
with libFuzzer, AFL, or another coverage-guided driver. It uses the smith engine
and reports insufficient entropy or rejected candidates as errors. Successful
results have passed the same final validation and optional memtag augmentation.

## License

Project code is licensed under the [Apache License, Version 2.0](LICENSE).
Vendored upstream components retain their original licenses and copyright
notices; see [THIRD_PARTY.md](THIRD_PARTY.md).
