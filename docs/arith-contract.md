# Arithmetic Contract (bash `$(( ))` → backend semantics)

Normative spec for how the emitter lowers bash arithmetic to backends.
Audience: backend renderers (Perl, ESTree/JS, C) and runtime implementers
(sh2-namespace.mjs, the future C runtime). C ask A5a from
`backends/c/docs/backend-c-core-needs.md`.

Status: DRAFT. The JS path implements all of this today (PLAN.md v6); this
doc pins the semantics so every backend agrees *without a shared test*.

## 1. Value model

- Arithmetic is **64-bit signed integer** only (`i64` / JS `Number` safe-int
  / C `long long`). No floats, no bignums at the `$(( ))` level (the `bc`
  tier is separate — see §7).
- Every operand is first coerced: `$x` in arithmetic context parses its
  string value as an integer (`strtoll`-style); **empty or unparseable →
  0** (a documented bash quirk — `$(( $1 * 100 ))` with an unset positional
  is 0, even though bash syntax-errors on the same text).
- Result is an integer; comparison/logical operators yield exactly `0`/`1`.

## 2. Operators

| op | semantics | backend note |
|---|---|---|
| `+ - *` | wrap-around i64 (JS: `Number` — safe-int range; C: `long long` wraps) | native |
| `/` | integer division, **truncate toward zero** (bash and C agree) | native; zero divisor → §4 |
| `%` | modulo; **sign follows the dividend** (bash and C agree) | native; zero divisor → §4 |
| `**` | exponentiation, **right-associative** | JS `**`; C `pow` then truncate — overflow semantics must be pinned by a test |
| `< <= > >= == !=` | integer comparison → `0`/`1` | native |
| `&& \|\| !` | short-circuit logical on 0/nonzero → `0`/`1` | native |
| `& \| ^ << >> ~` | bitwise, wrap | native (rare in corpus) |
| `? :` | ternary | native |
| `+= -= *= /= %=` | compound assignment (variable form) | JS path: `sh2.assign` (setVar semantics) |
| `x++ x--` | post-increment/decrement | JS path: `sh2.assign` |

## 3. The numeric lift (provably-numeric sources)

The emitter admits a variable as **native numeric** (no runtime store) only
when every assignment to it is provably numeric (shir.rs:4085 — "the
numeric lift only admits sources that parse as integers"; shir.rs:4590 —
"assignment is provably numeric (a `$((...))` expression without `/`/`%`)").
Rules a backend can rely on:

- A native-numeric variable is only ever assigned integer literals,
  integer-typed variables, or `$((...))` without division/modulo.
- Anything else (string assignments, unquoted captures, `read` into the
  var) forces the var onto the runtime store path (JS: `sh2.getVar`/
  `sh2.setVar`).
- This is the C backend's type source: native-numeric → C `long long`;
  store-path → C string/var-store (`backends/c/docs/backend-c-core-needs.md`
  §3, ask A2).

## 4. Division/modulo by zero

Bash aborts the **whole expansion** (`bash: division by 0`); the
`$((...))` result is the expansion failure, not a value. Backends must
throw/abort, never produce a number:

- JS path: `sh2.idiv(a, b)` / `sh2.imod(a, b)` throw inside
  `sh2.arithEval`.
- C path: `sh2_idiv`/`sh2_imod` runtime functions with the same abort
  (or a C `SIGFPE` trap converted to the same abort semantics).

## 5. Non-numeric operands in tests

`[ "$x" -gt 5 ]` with a non-numeric `$x` → bash prints `integer expression
expected` and the **whole test is false**. JS path: `Number.isNaN` guard
(whitelisted); C path: `strtoll` + endptr check → false (sh2.guard).

## 6. Constant folding

Shared `optimize_stmts` folds provably-constant `$((...))` → `Int` literals
and `Int BinOp`s (M6, PLAN.md §7) with a Rust evaluator (digits,
`+ - * / %`, parens). Backends receive the folded form; the evaluator's
semantics are this contract (truncate-toward-zero, sign-follows-dividend).

## 7. The bc tier (arbitrary precision)

`bc -l` / `bc <<< 'sqrt(...)'` lowers to the `sh2.bcSqrt` family (native
wasm core, SH2_BC_NATIVE=exact). C has no free bignum: **keep these on the
spawn path** (`exec("bc", ...)`) — the only genuinely spawn-required family
in the corpus. Errors (negative sqrt, div-by-zero, parse) return empty
stdout (bc's no-stdout-on-error), never a number.

## 8. What a backend must test

A backend claiming arithmetic parity must pass at minimum:
1. `$((7/2))` → `3`; `$((-7/2))` → `-3` (truncation, not floor)
2. `$((-7%2))` → `-1`; `$((7%-2))` → `1` (dividend sign)
3. `$((2**3**2))` → `512` (right-assoc)
4. `$((x))` with `x` unset → `0`; `x='abc'` → `0`
5. `$((1/0))` → whole expansion aborts (no stdout)
6. `[ "$x" -gt 5 ]` with `x='abc'` → false, stderr `integer expression expected`
7. `i=0; i=$((i+1))` in a loop → native path, no store round-trip
