# Integer homes: true64 vs bigint, per source language

The default exact-arithmetic home depends on the SOURCE language's own
semantics, not on a global preference. Bash arithmetic is int64
wraparound (`$((2**63))` = `-9223372036854775808`), so faithfulness for
shell input means i64 — bignum would be a behavior change, not extra
safety. Python/JS-origin programs have native bignum semantics, so
narrowing THEM is the behavior change.

## The matrix

| Source | Default flag | Exact home | Narrow-on-proof |
|---|---|---|---|
| shell (bash/zsh/ksh/dash) | `--true64` | i64 wraparound (bash-exact) | yes — proven ≤2^53 vars stay Number |
| Python / JS origin | `--bigint` | bignum everywhere | yes — same verdict machinery |
| C origin | typed verdicts | the declared `var_types` type | n/a (already typed) |

`--bigint` implies true64 semantics (a superset): i64 values are exact
inside it; only >64-bit magnitudes distinguish it.

## Mechanism (shared, shIR-level)

1. **Verdict prior flip**: `analyze_var_ranges`/`analyze_true64` seed
   Unknown-as-wide instead of Unknown-as-Number. The existing loop
   fixpoints and the numeric-lift decide which vars PROVE narrow; the
   rest take the exact home. One analysis, every backend reads the same
   verdicts (`var_types` + the true64 statics).
2. **Boundary exactness**: string sources (argv, `read`, captures) parse
   EXACTLY at the binding — JS `BigInt(str)` (never `Number(str)`), C
   `strtoll`, Python native. This kills the argv-rounding hole the
   default path had.
3. **Per-backend exact homes**: JS BigInt values / BigInt64Array slots
   (the true64 machinery, already built); Python native `int` (free);
   C `__int128`; Go `math/big.Int`; Java `BigInteger`; Rust `i128`.
4. **Fast paths stay**: dual-loop versioning (SH2_DUAL_LOOPS) guards the
   hot loop — narrow arm when values fit, exact arm otherwise. The guard
   is what makes default-exact affordable.

## Status

- [x] true64 machinery (slots, values, Cast render) — shipped
- [x] dual-loop versioning — shipped (`SH2_DUAL_LOOPS=1` opt-in)
- [x] `--bigint` CLI wiring (maps to true64+exact boundary for the estree
      path; per-backend arms beyond JS land with the verdict flip)
- [ ] verdict prior flip (Unknown-as-wide) + boundary-exact bindings
- [ ] corpus gate → true64 default-on for shell input
