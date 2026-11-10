# wasm-fuzzer 0.2.0 prerelease

Generate valid pure Core Wasm under an exact feature policy. The default remains
Core 3.0; all 28 Core switches in wasm-tools 1.249.0 and the independent uwvm
memtag extension are selectable. Component/host-only switches are excluded.

- Separate float, GC-type, bulk-memory-opt and padded call_indirect controls.
- Experimental coverage includes legacy exceptions, shared-everything threads,
  stack switching, custom descriptors, memory control and compact imports.
- Exhaustive plans contain 12,288 official-proposal or 92,160 fine-grained Core
  configurations. The full 29-switch triplewise plan contains 7,348 configurations.
- Deterministic metadata/seed replay and bounded retries; every output passes
  exact-feature validation. Version 0.1 replay metadata requires version 0.1.
- Cross qualification checks unmodified wasm-tools, the actual official Core 3
  interpreter where supported, uwvm's production validator where supported,
  and wasm-git's exact policy, physical fields and bidirectional byte replay.
  Private memtag projections and unsupported consumer profiles are explicit.
- Ten platform archives match wasm-git, with SHA256SUMS and bundled licenses.
  CI builds/packages; publication requires local qualification of the exact
  commit for at least 30 minutes inside the existing 64 GiB, zero-swap cgroup.

Generated modules are valid, not necessarily safe to instantiate or trap-free.
The qualification pipeline validates and replays bytes without executing guests.

Qualification on 2026-11-11 completed **1,800.54 seconds**, **40,795** cross
checks over **39,272** distinct modules and **19,620** feature configurations,
with no failure. Every check passed exact wasm-tools/wasm-git policy validation,
seed replay and four complete byte reconstructions (audit/patch, both directions).
The official interpreter accepted 27,660 originals and 378 typed projections;
12,757 experimental profiles were explicitly unsupported. uwvm accepted 26,646
CLI originals and 1,813 imported-module originals through its production API;
12,336 profiles were explicitly unsupported. Unmodified wasm-tools accepted
33,172 originals and 7,623 typed memtag projections. No OOM event occurred.
Separately, all 92,160 fine-grained Core configurations generated two valid
modules each (184,320 modules). Native and WASIp1 MVP/Core2 release smoke passed.
These finite results do not establish every possible module or proposal revision.
