# Optimization levels: speed, size, readability

Three axes, one mechanism. This document defines the level taxonomy, the
pass classification behind it, and the testing contract per level. Status:
**partially implemented** — `lastexit-call-sites` (cleanup class) and
dual-loop versioning (speed class, `SH2_DUAL_LOOPS=1` opt-in until
corpus-verified at scale) exist; the preset→gate plumbing and the
sign/range analyses are still open work.

## The axes and the firewall

- **Speed** (cycles), **size** (bytes), **readability** (can a human audit
  the output) are traded against each other by *presets*.
- **Semantic flags** (`--true64`, `--source-lang`, bigint regimes) are NOT
  levels: they change observable behavior (overflow exactness) and stay a
  separate, explicit dimension. See `true64.md`.
- **Firewall (non-negotiable): levels change bytes and cycles, never
  observable behavior.** stdout, exit codes, stderr ordering — identical
  at every level for every corpus program. A level that changes behavior
  is a bug, full stop. The corpus gate is the enforcer (see Testing).

## Levels

| Level | Meaning | Build cost | Use |
|---|---|---|---|
| `-O0` | Literal lowering, fastest transpile. Cleanup off, analyses skipped. Correspondence-readable (maps line-to-line to input) but noisy. | Lowest | Debugging the transpiler itself |
| `-Og` | **Readability mode**: `-O0` + full cleanup, minus everything that obscures (no inlining/versioning/unrolling, no intent-hiding strength reduction, names and structure preserved). The mode you audit translations in. | Medium | Human review, diffing |
| `-O2` (default) | Today's pipeline, byte-stable. Balanced. | Medium | Everything (default) |
| `-Os` | Size-first, speed-second: no duplication of any kind (single-wide loops, no unrolling/versioning/inlining-for-speed), shared helpers, keep shrink-and-speed peepholes. | Medium | Shipping where bytes matter |
| `-Oz` | Size at (almost) any cost: `-Os` + outlining repeated bodies into helpers, prefer compact lowering even when slower (runtime call over inlined native), minimal temps. Build only if a size-constrained consumer appears (browser bundle?); otherwise defined-and-deferred. | Medium–high | Size-constrained shipping |
| `-O3` / speed | Versioning (entry-guarded dual loops), unrolling/peeling where proven, aggressive inlining, strength reduction. | Highest | Hot programs, benchmarks |

Levels are **presets**, not orthogonal flags: `-Oz`-readable is a
contradiction (outlining/minification hurts readability) and is not
offered. Levels also gate **analysis budget**: `-O0` skips the expensive
fixpoints (range/provability) — that is half of what makes it fast, the
rest being skipped rewrites.

## Pass classification (build checklist)

**Cleanup** — smaller *and* faster *and* clearer; no tradeoff. On at
`-Og` and up (and `-O2` default). The only reason to skip is
transpile-time (`-O0`) or byte-exact frontend debugging:
`dead_store_elim`, `dead_fn_elim`, `redundant_store_elim`,
`copy_propagation`, `const_capture_fold`, `const_condition_elim`,
`merge_init_assignments`, `arith_identity`, `test_simplification`,
`string_accumulator`, the `lastexit-dead` family (incl.
`lastexit-call-sites`), `escape-classes` (analysis).

**Enabling** — canonicalizations that change shape substantially but unlock
downstream speed/size wins; on everywhere except `-O0`:
`counted_while_forinit`, `for_recovery`, `seq_range_for`,
`arith_forms`, `ternary_desugar`, `shir_pipeline_native`,
`shir_native_stmt`, `process_subst`, `for_recovery`, `test_lowering`,
`loop_return_lift`, `echo_return`, `direct_calls`, `inline_pure_fns`
(as canonicalization; its *aggressive* use is speed-class).

**Speed-only** (refused at `-Os`/`-Oz`/`-Og`): loop entry-guard dual
versioning (§Width below), unrolling/peeling (none yet), aggressive
inlining, `div_mod_pow2` (obscures intent for `-Og`; speed/size win
elsewhere), `sync-ok-loops` BATCH checkpointing (adds code).

**Size-only** (`-Os`/`-Oz`): helper outlining (none yet), compact-lowering
selection. `-Oz` additionally prefers the smaller lowering when they
differ in speed.

## Width: narrow defaults, unsigned on proof, guards at entries

Bash arithmetic is signed int64 with wraparound. The lowering ladder,
per value, from cheapest:

1. **Proven range → native narrow.** Default narrow is **signed** (`i64`,
   `long`, JS Number below 2^53) — bash-identical on the proven range
   with no further obligation.
2. **Proven non-negative → unsigned** (`u64`, `uint64`, `unsigned long`).
   This is strictly more than a nicety (see table): it doubles the exact
   range to 2^64−1 AND degrades gracefully (wraparound is *defined* for
   unsigned, matching bash bit-for-bit, where signed overflow is UB in
   C / a debug panic in Rust). `u32`/`uint32_t` for proven [0, 2^32):
   narrower encoding on x86 (no REX prefix — a size win too), JS `>>> 0`.
3. **Unprovable → wide** (f64 with runtime guards where needed, BigInt /
   GMP / `__int128` per backend). Unknown inputs (argv, `read`,
   captures) are unprovable by construction.
4. **Unprovable-but-likely-small + hot → entry-guarded dual version**
   (speed-only): one magnitude check on already-materialized values at
   loop entry selects the narrow loop or the wide loop. Never a branch
   *inside* the loop. Refused under `-Os`/`-Oz`/`-Og` (it doubles loop
   code); the guard must imply the closure proof, read only materialized
   temps, and run in the wide domain (see the speed/speed-size discussion
   that motivated this).

Per-backend unsigned payoff (honest version):

| Backend | Unsigned win | Notes |
|---|---|---|
| C | **Large**: exact to 2^64−1 (vs 2^63−1 + UB above for `i64`); wrap is defined (bash-identical safety net); `u32` saves encoding bytes | Needs the nonneg proof; closure under guarded bounds |
| Rust | **Large**: same range argument; `wrapping_*` ops stay total (no debug-panic cliff) | Same proof |
| Go | **Medium**: defined wrap both ways anyway; win is the doubled range (`uint64`) | Same proof |
| Zig | **Medium**: like Rust (`u64` + wraparound operators) | Same proof |
| JS | **≈nil**: no uint64 arithmetic; `>>> 0` costs the same as `\| 0`; nonnegativity adds nothing beyond what `i32_provable` already proves | Skip versioning pressure here beyond i32 |
| Java | **Nil**: no unsigned-long arithmetic (`long` is it) | Skip |
| Python/Perl | **Nil**: transparent bignum/upgrade already | Skip entirely (no versioning) |

The proof obligation is **sign**, not full range — cheaper than interval
analysis: a NonNeg/Unknown lattice over straight-line code plus a
loop-carried fixpoint (same shape as `analyze_true64`'s loop fixpoints),
following the `i32_provable` verdict pattern (stable-path keyed,
analysis-only, gated). Literals, `RANDOM`-style bounded sources and
counter loops seed NonNeg; argv/`read`/captures/foreign calls seed
Unknown. `u32` falls out as a range-upper-bound special case of the same
machinery.

## Testing contract

- **Default (`-O2`) stays byte-pinned**: whatever golden-output tests
  exist keep passing unchanged.
- **Every other level is tested purely behaviorally**: the existing
  corpus gate runs at each level comparing stdout/exit against bash.
  This is the project's testing policy (observable behavior, not golden
  bytes) extended across the axis.
- New transforms arrive gated (the `DEBASHC_TRANSFORMS` convention:
  empty/unset = on) with structural tests first — appearance/behavior
  assertions, never "did not crash" — then the gate flip is verified
  green.

## Honest limits / open items

- The preset→gate plumbing does not exist yet; neither do unrolling,
  outlining, or the range/sign analyses. `i32_provable`,
  `analyze_true64` (+ loop fixpoints) and `lastexit-call-sites` are the
  models to copy.
- Dual-versioning needs the hotness signal to avoid versioning cold loops
  (counted loops, nesting depth, provably-runs — all already computed
  somewhere; they need one shared query point).
- `-Oz` is defined-and-deferred until a size-constrained consumer exists.
- Python/JS/Java backends largely sit out the width game (table above) —
  their speed work is elsewhere (call overhead, capture cost, runtime
  dispatch).
