# wasm-fuzzer

A Rust generator for **valid, feature-selected WebAssembly Core modules**, from
the MVP through Core 3.0, every Core feature switch in the pinned wasm-tools
1.263.0 validator, and the uwvm linear memory-tagging extension. It produces ordinary `.wasm` binaries. The default
configuration has no imports and requires no WASI, JavaScript, or component host.

Every successful generation passes an exact-feature validator before its bytes
are returned or written. Standard modules additionally pass the unmodified
upstream `wasmparser` validator. Rejected candidates have a bounded retry budget;
exhaustion is an error, never an empty-module substitution or an invalid output.

## Build

Rust 1.88 or later is required; the qualification toolchain is Rust 1.93.1.
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
Keep the source revision, build target and `Cargo.lock` with a corpus for long-term replay.
The seed contract is for the same build target; identical seed bytes across
32-bit and 64-bit generator architectures are not promised.
The seed-to-bytes contract includes the exact package version. Version 0.2.1
updates the upstream generator and rejects earlier replay metadata; retain the
matching binary when replaying older corpora.

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
| `empty` / `none` | No optional validator flags, including no floats or GC-managed types |
| `lime1` | The exact upstream `WasmFeatures::LIME1` embedding profile |
| `validator-default` / `default` | Pinned wasm-tools default Core flags (the implicit preset) |
| `all-core` / `all-known` | All 30 pinned Core validator switches, excluding memtag |
| `all` | All Core switches plus memtag |

The default matches **wasm-tools 1.263.0's default Core flags**, including threads,
wide arithmetic and compact imports. Use `--profile wasm3` for strictly Core 3.0.
Component flags and memtag are excluded from this default. wasm-git's
recognition boundary is separately **all-known**, regardless of the fuzzer's
selected compatibility policy.

All 31 switches are independently selectable with repeatable, comma-separated
`--enable` and `--disable`. `features` prints their dependency graph:

| Group | Switches |
| --- | --- |
| Core 3 proposals | `mutable-globals`, `saturating-float-to-int`, `sign-extension`, `multi-value`, `bulk-memory`, `reference-types`, `simd`, `relaxed-simd`, `tail-call`, `extended-const`, `function-references`, `gc`, `multi-memory`, `memory64`, `exceptions` |
| Fine-grained Core controls | `floats`, `gc-types`, `bulk-memory-opt`, `call-indirect-overlong` |
| Additional pinned Core proposals | `threads`, `wide-arithmetic`, `custom-page-sizes`, `legacy-exceptions`, `shared-everything-threads`, `memory-control`, `stack-switching`, `custom-descriptors`, `compact-imports`, `acquire-release-atomics`, `multibyte-array-access` |
| Private extension | `memtag` |

Enabling a proposal enables its prerequisites. GC needs function references,
which need reference types; reference types include `call-indirect-overlong`.
Bulk memory includes `bulk-memory-opt`; relaxed SIMD needs SIMD. Legacy
exceptions need exceptions, stack switching needs function references, and
custom descriptors and multibyte array access need GC. Disabling a prerequisite disables its dependents.
`gc-types` is an orthogonal embedding restriction: it is not silently enabled
by GC or stack switching; generation suppresses types forbidden by that control.

CLI ordering is configuration, optional preset, enables, then disables. JSON
feature sets are exact; missing dependencies and unknown fields are errors.
Enabled flags are permissions, not promises that every module uses every flag.
`memory64` includes table64. `bulk-memory-opt` permits memory.copy/fill without
passive segments. `call-indirect-overlong` permits a padded table-zero index
without general reference types. Float and GC-type restrictions are applied
inside the type-driven generator as well as at the final validation gate.

This is a **Core** fuzzer. Component Model `cm-*` switches and host-only JS/C API
features do not describe pure Core modules and are intentionally excluded.
The catalog is pinned; future grammar changes require an explicit version update.

## Feature-combination matrices

```sh
# From .tmp; each destination must be new.
target/release/wasm-fuzzer matrix --scope official --strength exhaustive --count 2 --out-dir official
target/release/wasm-fuzzer matrix --scope core --strength exhaustive --count 2 --out-dir core
target/release/wasm-fuzzer matrix --scope all --strength triplewise --imports --count 2 --out-dir all
target/release/wasm-fuzzer matrix --scope all --strength pairwise --plan-only --out-dir plan
```

`official` enumerates the 15 Core 3 proposal toggles on an MVP baseline:
**12,288** dependency-closed configurations. `core` also varies the four embedding
controls from the empty baseline: **92,160** configurations. `all` includes the
experimental proposals and memtag. Pairwise/triplewise plans cover every feasible
on/off assignment for every pair/triple in both sparse and dense contexts; the
31-switch triplewise plan contains **8,886** configurations. It is not exhaustive
over all 31 switches. Exhaustive plans are capped at 20 dimensions to prevent
accidental exponential allocation.

Each plan records its exact features in `matrix.json`; generated subdirectories
use the ordinary corpus/manifest/replay format. Every emitted module still passes
the exact-feature gate. The mixed engine uses both structured and smith cases
when `--count 2 --seed 0` is selected.

## Generation engines and controls

`--engine mixed` is the default: one seed in four uses the structured engine,
and the others use `wasm-smith`. `--engine smith` selects the general type-driven
generator. It covers recursive and subtype groups, typed operands, structured
control, scalar and vector instructions, references, GC, tables, memories,
segments, tags, and exceptions. `--engine structured` supplies randomized
coverage cases, including function references without GC, legacy exceptions,
continuations, descriptors, shared globals, memory.discard, and memtag. Compact
import groups and padded call_indirect immediates are encoded without changing
entity indices. Some experimental instruction families are supplied by the
structured engine; use mixed mode for proposal coverage and random type graphs.

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

`tools/cross-test.py` extends this pipeline to every pinned Core switch and
matrix directory. Build the independent, unmodified wasm-tools CLI first:

```sh
tools/run-in-cgroup.sh bash ../tools/build-wasm-tools.sh
tools/run-in-cgroup.sh python3 ../tools/cross-test.py official all \
  --generator target/release/wasm-fuzzer --wasm-tools target/release/wasm-tools \
  --official reference/wasm --uwvm /absolute/path/to/uwvm2-ros \
  --uwvm-pure uwvm-validator/validate --wasm-git /absolute/path/to/wasm-git \
  --duration 1800 --heartbeat 600 --workers 4 --output qualification
```

Every case must pass exact flags in wasm-tools and wasm-git, deterministic seed
replay, maximal-grammar field-span inspection, and **100% forward and reverse
byte replay** through both audit edits and serialized patches. The minimum wall
clock is 30 minutes, with a complete corpus traversal required before success.
Status is written every 30 seconds; explicit heartbeats occur every 10 minutes.
Failures stop qualification and retain the input, command output and diagnostics.
Tool hashes are checked again at completion to prevent mid-run binary changes.

`tools/qualify.py` is the release gate. It runs valid-only generation, random
hypothetical syntax updates, long history replay and all regressions together.
The first three campaigns each run **for at least 30 minutes**:

```sh
tools/run-in-cgroup.sh target/release/wasm-fuzzer generate --profile all --imports \
  --seed 314159 --count 32 --out-dir upgrade-seeds
tools/run-in-cgroup.sh python3 ../tools/build-regressions.py \
  --wasm-git-root ../../wasm-git --output regression-artifacts.json
tools/run-in-cgroup.sh python3 ../tools/qualify.py official all \
  --generator target/release/wasm-fuzzer --wasm-tools target/release/wasm-tools \
  --official reference/wasm --uwvm /absolute/path/to/uwvm2-ros \
  --uwvm-pure uwvm-validator/validate --wasm-git /absolute/path/to/candidate/wasm-git \
  --baseline-wasm-git /absolute/path/to/baseline/wasm-git \
  --syntax-checker ../../wasm-git/tools/check-syntax-upgrade.py \
  --history-checker ../../wasm-git/tools/check-history-replay.py \
  --regression-artifacts regression-artifacts.json --wasm-git-root ../../wasm-git \
  --spec-root reference/spec-943c5df07e84f11d2b83078964df5812530afbd2/test/core \
  --uwvm-source /absolute/path/to/uwvm2-ros \
  --mutation-corpus upgrade-seeds --random-cases 2048 --seed 2718281828 \
  --duration 1800 --heartbeat 600 --workers 8 --output paired-qualification
```

The syntax campaign cycles through the module version and every Core section
family, samples nested physical fields, and applies reproducible random edits.
It also requires all **157 reserved/extension admission-site families** in
wasm-git's reviewed catalog: section IDs, type constructors, import/export kinds,
flags, opcode/subopcode namespaces, memory operands, orderings, handlers, custom
subsections and framing fields. Byte selectors receive all 256 values in seeded
random order; wider fields receive bit/complement patterns, boundaries and
random values. Reports verify every required site and distinct selector count.
See [the syntax evolution contract](https://github.com/UlteSoft/wasm-git/blob/main/documents/SYNTAX_EVOLUTION.eu_US.md)
for the inventory and the distinction between admission-site coverage and
unimplemented future semantics. Smoke-only sampling cannot pass the release gate.
Each update must preserve 100% forward/reverse patch replay, source integrity
checks, bounded audit spans, and exact raw-byte evidence. Mutants may be future
syntax or malformed under today's grammar; this separate robustness campaign
does not weaken the generator's valid-only guarantee. All campaigns must pass,
and a failure stops the combined gate. Reports include seeds, selected fields,
replacement bytes, hashes and consumer limitations; intermediates stay in `.tmp`.
The wasm-git patch compatibility baseline starts at version 0.1.4, so future
candidates must be checked against the released baseline and subsequent versions.

The combined gate also requires a **2,048-step continuous patch chain**, replayed
from its root to its tip and completely reversed by both binaries. Every stored
revision is checked after GC/repacking, cloning, bundle import and independent
forking. Sixteen forks, a nested fork, divergent branch tips, source removal and
128-step cherry-pick/revert/rebase queues exercise history and storage behavior.
An independently encoded expected module checks the two-parent merge result.
Historical reconstruction continues throughout the timed window.

All compiled Rust regressions run from scratch storage. The 13 opt-in tests are
mandatory too: the pinned official corpus, real uwvm examples/workflows, extended
byte fuzz, and the 600-second memtag and language/debug campaigns. A new ignored
test requires an explicit runner entry; it cannot disappear from acceptance.
Only the fixed pure preload fixtures in the existing uwvm workflows are executed;
generated fuzz guests and ROS network modules are not executed.

The official interpreter has no fine-grained feature switches and supports Core
3, not every experimental proposal. Unsupported profiles are reported separately
and never counted as official passes. The unmodified wasm-tools CLI checks all
30 pinned Core switches with `--features=-all,...` and no inherited defaults.

Private memtag instructions are rejected by unmodified wasm-tools. For memtag,
a typed projection replaces each instruction with input drops and typed zero
results. This preserves **typing, not semantics**; projection success is never
reported as official acceptance of private bytes. wasm-git and uwvm validate
original memtag bytes, including static globals metadata.

The uwvm adapter records these coarser flag mappings: GC local-global
initializers need extended-const; explicit multi-memory data segments and
bulk-memory-opt need bulk-memory; bulk-memory table-zero segments need
multiple-tables; padded call_indirect needs reference-types; function-references
return_call_ref needs tail-call. Floats/gc-types restrictions remain enforced by
the preceding exact gates. uwvm-unsupported experimental profiles are counted as
unsupported, not passed. `tools/verify.py` remains the original Core 3 + memtag
runner; use `cross-test.py` for the full feature matrix.

Invalid controls prove that validators check types, not just framing. Reports
contain exact tool hashes, cgroup limits, elapsed time, feature mappings, and
per-case receipts. All intermediate evidence stays in ignored `.tmp` storage.

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
  bash ../tools/check.sh
```

Tests cover standard profiles, proposal-by-proposal enablement, deterministic
replay, feature swarms, every section/category switch, boundary configurations,
fuel, all memtag signatures at both address widths, imported-memory indexing,
malformed extension encodings, metadata range overlap, CLI errors, and overwrite
refusal. External three-gate tests use the separate command above.

Version **0.2.1** passed the full combined gate in **2,084.63 seconds** (34 minutes
45 seconds), with no failures or OOM events. All 42,348 corpus inputs and 21,158
distinct feature configurations completed exact-feature validation and byte-exact
forward/reverse replay. A separate exhaustive fine-grained Core enumeration
produced 184,320 validated modules.

The gate covered all 157 admission-site families (28,800 selector probes), 164
fixed syntax cases and 2,048 random updates across the module header, every Core
section and new section IDs: 248,096 exact patch applications. Long histories
passed 12,288 patch applications and 10,410 historical checkouts, including all
2,048 chain edges, 16 forks, nested forks and 128-step replay queues. All 1,554
regular regressions and 13 opt-in tests passed (1,582 test executions including
16 language workers). Both timed fuzzers completed their full 600 seconds.

The unmodified wasm-tools validator accepted 33,462 originals and 8,886 typed
memtag projections. The official Core interpreter accepted 26,624 originals
and 366 projections; 15,358 unsupported profiles were recorded separately.
uwvm accepted 25,655 originals through its CLI and 1,741 through its production
pure validation adapter; 14,952 unsupported profiles were recorded separately.
These projections do not certify private memtag instruction semantics. All
machine-readable evidence and intermediate artifacts remain in ignored `.tmp`.

## Binary releases and CI/CD

[GitHub Releases](https://github.com/UlteSoft/wasm-fuzzer/releases) use the same
10 platform labels as wasm-git: static-musl Linux x86_64/AArch64/ARMv7/RV64GC,
Windows x86_64/ARM64 with static MSVC CRT, macOS x86_64/ARM64, and WASIp1
MVP/Core2. WASI binaries are CLI programs with Preview 1 imports; the modules
they generate remain pure Core Wasm unless imports are explicitly requested.

Rust 1.93.1 is pinned. WASI builds rebuild std and libc for the selected target
profile. Each archive contains only the binary, English documentation, and
licenses. `SHA256SUMS` covers exactly ten archives. Windows packaging inspects
PE imports to reject an external VC runtime dependency.

Hosted CI performs formatting, linting, compilation and packaging. Runtime
acceptance stays in the existing 64 GiB cgroup. Tag publication requires a
successful `qualification/cgroup-64gib` commit status for the exact commit, set
only after the local 30-minute cross campaign passes. Build artifacts and
qualification evidence are never committed. These are cross builds, not a
claim of executing Windows/macOS binaries on the Linux qualification host.

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
