# --true64: true 64-bit bash arithmetic (off by default)

The bash → ESTree/JS translation lowers shell arithmetic to JS Numbers.
JS Numbers are exact only to ±2^53 — a bash script computing beyond that
silently corrupts:

```sh
$ x=9007199254740992; x=$((x + 1)); echo "$x"
9007199254740993        # bash: exact int64
$ debashc --shir-in-estree <a1> | node estree-runner.mjs
9007199254740992        # default: the +1 vanished (Number rounding)
```

`--true64` fixes this: out-of-±2^53 numeric vars lower to true 64-bit
arithmetic. **Off by default** (the default path is unchanged).

## Usage

```
debashc --shir-in-estree --true64 <file.a1>   # A1 -> ESTree, true 64-bit
```

The flag may sit between the mode and the filename. Only the
`--shir-in-estree` (bash → JS) path honors it; the C frontend's typed
path (`var_types` Int64/UInt64) is already exact.

## The lowering (per var, from `analyze_true64`)

`analyze_true64` classifies every numeric var by its conservative range
(`analyze_var_ranges`, with loop fixpoints):

| var's proven range | lowering | cost |
|---|---|---|
| provably inside ±2^53 | **JS Number** (the default path — exact there) | ~1 ns |
| escapes ±2^53, a **self-RMW accumulator chain in a loop** written only via plain single-target Assigns (no function-locals, no indexed writes) | **BigInt64Array slot** — `const __t64 = new BigInt64Array(N)`, reads/writes `__t64[k]` | ~1.8 ns/op (V8's native int64 element arithmetic) |
| escapes ±2^53, anything else | **BigInt value** (Int64: BigInt read coercion, `asIntN(64, …)` wrap on assign) | ~5–25 ns ops, exact |

### The slot heuristic (BinInt64.md §7, implemented)

A var is slot-worthy iff ALL of:

1. **self-RMW** — every write is `x = x op e`, `x op= e`, `x++`/`x--`
   (the fast path *is* load → op → store; the value never leaves the array);
2. **in a loop** — the RMW is executed many times (the win is
   per-execution);
3. **only plain single-target Assign writes** — a string-init/`local`/
   `declare`/indexed write desyncs the slot from the value's other home
   (a function-local's `let i = init` bakes the init into the
   declaration, bypassing the slot branch — function-body vars are
   excluded wholesale).

Vars proven inside ±2^53 always stay Numbers (1 ns beats 1.8 ns). The
operand set is closed over the loop (a slot's arith operands are slotted
or BigInt-wrapped — mixed i64/Number ops would throw).

## Rendering details

- **Arith leaf wrapping** (`wrap_true64_arith_ast`): Num leaves →
  `Cast(Int64, Num)` (exact `BigInt("N")`), non-slot Var leaves →
  `Cast(Int64, Var)` (BigInt coercion), slot reads stay RAW `__t64[k]`
  (the native fast path). Applied to IR arith trees AND the arith parsed
  from test-string `$(( ))` operands (render-time, `num_operand`).
- **Test operands**: BigInt equality ops wrap the operand in `Number()`
  (`0n === 0` is FALSE — strict equality doesn't coerce BigInt/Number;
  a nonzero BigInt never rounds to 0, so the wrap is exact for
  `=== 0`/small constants). Relational ops keep the raw BigInt (exact —
  BigInt vs Number comparisons are legal).
- **Zero divisors**: `BigInt % 0` throws where `Number % 0` → NaN; bash
  aborts the expansion (test false). Div/mod test operands get per-
  divisor guards: `(d === BigInt("0") ? NaN : <arith>)`.
- **Observation points** (echo/printf/interpolation/test/getVar) read
  the slot via the native element (`BigInt` stringifies exactly); printf
  `%d`-family skips `parseInt(BigInt)` (throws) via the existing
  BigInt-arg path.
- **IncDec on slots**: native `++__t64[k]` (the 1.1 ns/op
  `mem[i] += 1` fast path, wrap native).

## Verified (gcc/bash oracle)

| case | default | `--true64` | bash |
|---|---|---|---|
| `x=2^53; x=$((x+1))` | …992 | **…993** | 993 |
| `n*3` loop past 2^63−1 | …668 | **16677181699666569** | 16677181699666569 |
| accumulator loop | ok (small) | `const __t64 = new BigInt64Array(1); __t64[0] = __t64[0] + BigInt("2")` | ok |

Gates: c-sh-go 86/86, core lib tests 240 pass, default estree output
unchanged (the statics are empty without the flag). Unit test:
`range_analysis_tests::true64_slot_selection`.

## Design notes / honest limits

- The slot fast path is RMW-only: general i64 expressions (non-RMW
  writes, values observed every iteration) get no array benefit — they
  lower to BigInt values (the C-path lowering), which are exact with
  modest cost. See BinInt64.md §7 for the full cost model.
- Slot vars are per-program; the array is `const __t64` at module top.
- The corpus gate (`fail-estree`) passes no CLI flags — `--true64` is
  verified via targeted scripts + the unit test, not the corpus runner.
- Pre-existing, unrelated: a plain `return` inside a `whileLoopSync`
  body never exits the loop (051_primes.sh hangs in BOTH the default
  and `--true64` paths) — a return-in-loop lowering gap, not a true64
  issue.
