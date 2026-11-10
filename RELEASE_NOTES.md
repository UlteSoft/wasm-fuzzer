# wasm-fuzzer 0.2.1 prerelease

Generate valid pure Core Wasm with all 30 Core switches from wasm-tools 1.263.0
and the independent uwvm memtag extension. The implicit preset matches the
pinned official validator's default Core flags; explicit MVP/Core 1/2/3 presets
retain their selected specification boundary.

- Add acquire-release atomics (explicit memory orderings and pause) and multibyte
  GC-array memory access, with real witnesses and exact enable/disable gates.
- Include legacy exceptions, shared-everything threads, stack switching,
  custom descriptors, memory control, compact imports and all embedding controls.
- Official proposal enumeration: 12,288 configurations. Fine-grained Core:
  92,160 configurations. The complete 31-switch triplewise plan: 8,886.
- Update wasm-smith, wasmparser, wasm-encoder and WAT to the same upstream
  release. Preserve local patches and upstream licensing. Memtag's private
  validator flag uses bit 63 to avoid upstream component flags.
- Replay requires this exact version and build target. Every successful output
  passes validation; retries are bounded and failure is explicit.
- Ten platform archives match wasm-git, with SHA256SUMS and bundled licenses.
  CI builds/packages; publication requires at least 30 minutes of local paired
  qualification in the existing 64 GiB, zero-swap cgroup. The combined gate runs
  exact-feature validation/replay, randomized hypothetical syntax updates, long
  history replay and every regular and opt-in regression together.
- Require all 157 reserved/extension admission-site families, including every
  byte selector and seeded wider-field boundaries; preserve exact bytes and
  source integrity in both patch directions.
- Add a 2,048-edge continuous forward/reverse patch chain, 16 independent forks,
  a nested fork, source-removal recovery, and 128-step cherry-pick/revert/rebase
  queues. Check every revision after GC, repacking, cloning and bundle import.
- Run all 13 opt-in regressions, including the official Core corpus, real uwvm
  workflows, extended byte fuzz and both complete 600-second timed campaigns.

Generated modules are valid, not necessarily safe to instantiate or trap-free.
Qualification validates and replays bytes without executing generated guests.
The official Core interpreter and uwvm report unsupported experimental profiles
separately. Typed memtag projections do not certify private instruction semantics.

Acceptance completed in 2,084.63 seconds with no failures or OOM events:
42,348 corpus inputs, 28,800 admission-site probes, 2,048 random section updates,
248,096 syntax patch applications, 12,288 long-history patch applications and
10,410 historical checkouts. All 1,554 regular tests and 13 opt-in tests passed.
The 600-second memtag campaign validated 202,370 legal modules and replayed
404,740 patches; the 600-second language/debug campaign checked 1,984,929 cases.
See README.md for exact original/projection/unsupported validator counts.
