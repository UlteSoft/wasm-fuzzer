# Local reader provenance

This is wasmparser 0.249.0 as extended by wasm-git, including its overflow-safe
reader bounds fix and uwvm packed-wire memtag grammar, feature gate and typed
operator validation. It is not an unmodified upstream release.

The source was copied from wasm-git based at commit
`42e2c5e48ec657853b8d435342e8fdb617101538`. All upstream licensing files remain.
See [the project provenance record](../../THIRD_PARTY.md) for archive hashes,
upstream revisions and the independent standard-validator dependency.

Memtag globals v2 metadata validation is implemented by the enclosing project;
this reader alone, like upstream wasmparser, treats custom payloads as opaque.
