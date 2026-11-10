# Sources and local patches

## wasm-smith 0.249.0

- Upstream: <https://github.com/bytecodealliance/wasm-tools/tree/efeee7cafa5a3dcdddb5aab27bc0c05f279929ed/crates/wasm-smith>
- Crate archive SHA-256: `bbd31d0a3861a8eb5c19024a1ee39a7c805151c44074a19a416599ff25b0540a`.
- Vendored in `vendor/wasm-smith`, with original licensing files retained.
- Local patch: add `Config::allow_global_mixing` and propagate it into the code
  builder. When false, dropping an operand does not implicitly create/export
  mutable globals. Upstream behavior remains the default. This project sets it
  to false because injected mutable exports break MVP validation and hidden
  globals violate section switches. No type-validation rule is relaxed.

## wasmparser 0.249.0, memtag reader

- Upstream: <https://github.com/bytecodealliance/wasm-tools/tree/efeee7cafa5a3dcdddb5aab27bc0c05f279929ed/crates/wasmparser>
- Original crate SHA-256: `30538cae9a794215f490b532df01c557e2e2bfac92569482554acd0992a102ea`.
- The local reader was copied from the `wasm-git` working tree based at commit
  `42e2c5e48ec657853b8d435342e8fdb617101538`, including its memtag changes.
- Vendored in `vendor/wasmparser-memtag` and imported with an explicit Cargo
  dependency alias. The unmodified registry wasmparser is a separate dependency.
- Retained modifications include overflow-safe reader bounds checks, the
  independent `MEMTAG_EXTENSION` flag, the registered packed-wire instructions,
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
