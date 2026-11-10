# Sources and local patches

## wasm-smith 0.263.0

- Upstream: <https://github.com/bytecodealliance/wasm-tools/tree/7a2fa422cdf9aeeaf488a2acac8bc94a078bfa32/crates/wasm-smith>
- Crate archive SHA-256: `1102593f74a5b8b496683ea553f55a899a54461a5ad370d1a9eafd32e698a29d`.
- Vendored in `vendor/wasm-smith`, with original licensing files retained.
- Local patch: add `Config::allow_global_mixing` and propagate it into the code
  builder. When false, dropping an operand does not implicitly create/export
  mutable globals. Upstream behavior remains the default. This project sets it
  to false because injected mutable exports break MVP validation and hidden
  globals violate section switches. No type-validation rule is relaxed.
- Additional local controls separate function references from GC, bulk-memory-opt
  from passive segments, and GC-managed heap types from function references.
  Floats and all four embedding controls propagate into the generated feature
  policy. Typed calls, reference operations and table initializers honor these
  controls. Every result is checked by an unmodified upstream validator.
- Release packaging and the WASIp1 std/libc rebuild procedure are adapted from
  Apache-2.0 licensed wasm-git release tooling.

## wasmparser 0.263.0, memtag reader

- Upstream: <https://github.com/bytecodealliance/wasm-tools/tree/7a2fa422cdf9aeeaf488a2acac8bc94a078bfa32/crates/wasmparser>
- Original crate SHA-256: `4f5fa0a054ae153606df820eb0a84213e28cda41bbe04ba2628e71b6ba100fcb`.
- The local reader was copied from the `wasm-git` working tree based at commit
  `42e2c5e48ec657853b8d435342e8fdb617101538`, including its memtag changes, then ported onto upstream 0.263.0. Bit 63 avoids
  collision with the new upstream component feature bits.
- Vendored in `vendor/wasmparser-memtag` and imported with an explicit Cargo
  dependency alias. The unmodified registry wasmparser is a separate dependency.
- The reader bounds fix is now supplied by upstream. Local modifications are the
  independent `MEMTAG_EXTENSION` flag at bit 63, the registered packed-wire instructions,
  and memory-index/address-width-aware operand validation. Reserved operations,
  unknown domains/versions and malformed immediates remain errors.
- This project's globals v2 validator is separate in `src/memtag.rs`; it checks
  structure, range bounds and allocation-granule ownership.

Both crates retain their original Apache-2.0, Apache-2.0-with-LLVM-exception,
and MIT licensing alternatives. Their license files and upstream VCS metadata
are included. Project-owned source is Apache-2.0.

## Behavioral references

- Official Core specification: <https://webassembly.github.io/spec/core/>
- Official reference interpreter: <https://github.com/WebAssembly/spec/tree/943c5df07e84f11d2b83078964df5812530afbd2/interpreter>
- Core 3 feature overview: <https://webassembly.org/news/2025-09-17-wasm-3.0/>
- uwvm memtag contract: <https://github.com/UlteSoft/uwvm2-ros/blob/main/documents/runtime/memtag-extension.md>
- wasm-git: <https://github.com/UlteSoft/wasm-git>

The memtag contract uses packed instruction version 1, linear domain 1, and
globals metadata version 2. It deliberately rejects historical FC/multi-field
encodings and the reserved `memtag.add` operation. The local checked-in reader
and explicit regression tests define the exact supported snapshot.
