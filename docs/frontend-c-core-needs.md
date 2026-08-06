# C Frontend: What It Needs From ShIR

Status: DRAFT. The FRONTEND mirror of `backends/c/docs/backend-c-core-needs.md`
and the `docs/backend-*-core-needs.md` family: those ask "what does the
C *backend* want from core"; this asks "what does C as a *source language*
want from core". Grounding: the current IR surface (`src/ir.rs` —
`BinOpKind` set, `IrExpr`/`IrStmt` node inventories), the A2 type verdict
(`IrType {Int, Str, Any}`), and the shared ShIR JSON contract (A1, landed).

## 0. TL;DR

C is the nearest thing to a universal *target* ("lots of things compile
to C": GHC, Nim, Zig's C backend, DSLs) — so C is a uniquely valuable
*source*: **any language that targets C becomes a shell-family transpile
target** (C → ShIR → JS/Perl/Python/Lua/Rust/Zig). The shell IR is
~80% of what a C frontend needs (imperative control flow, fds, the store);
the gaps are exactly where shell and C differ: **types, pointers,
goto/switch, casts**. FOR NOW the tractable target is a **portable C
subset** with defined semantics (no UB, bounded pointers); full C is,
for now, a research project. This boundary is deliberately provisional
— the ESTree worker OWNS the core and is free to propose clever
solutions (pointer emulation, defined-UB semantics, better
control-flow transforms) that lift parts of the subset; the corpus +
round-trip oracle judge. The extension is one-directional: C adds type/memory/control
nodes; the shell-command nodes (`Pipeline`/`Redirect`/`Capture`/
`Subshell`/`Glob`) stay unused by the C frontend.

```
shell ──► Shell AST ──┐
                      ├──► ShIR (IrProgram) ──► [JS, Perl, Python, Lua, Rust, Zig, C] backends
 C ────► C-subset AST ┘         ▲
   (new frontend)               │ extended IR (types, memory, control)
```

## 1. The framing — why C as a frontend

- **Reach**: every language with a C backend (GHC Haskell, Nim, many
  DSLs, embedded C-style dialects) gains the whole backend family for
  free — one C frontend unlocks all seven targets.
- **The loop closes**: shell → ShIR → C (the C backend, being built) and
  C → ShIR → shell both flow through the same contract — C and shell
  become *exchangeable frontends* over one IR.
- **The round-trip oracle**: C → ShIR → C must reproduce the input — a
  compiler-identity test with the same discipline as the corpus gate
  (determinism byte-identical, structural gate green).
- **The honesty gate**: the value depends entirely on the subset. A
  defined-semantics portable subset is a compiler; arbitrary C (UB,
  raw pointers, setjmp/longjmp, varargs) is a research project. The
  subset IS the design.

## 2. The extension inventory (what the current IR lacks)

Verified against `src/ir.rs`:

| present today | missing for C |
|---|---|
| `IrStmt`: If/While/DoWhile/For/Case/break/continue/return/Block/Declare/Assign/Output/WriteFile/Exec/Pipeline/Redirect/Capture/Subshell/Background/Function | **goto/labels**, **C-switch fall-through**, typed function signatures, storage classes (static/extern/typedef/enum) |
| `IrExpr`: Int/Str/Var/Index/BinOp/Call/MethodCall/Ternary/DefinedOr/Interpolate/Capture/Range/Arith/Array/Ident/Bool/Json/Object/Arrow | **Cast**, **Sizeof**, **Member** (struct fields), **Comma**, **AddrOf/Deref** (subset) |
| `BinOpKind`: Add..Mod/Pow/Concat/Eq..Ge/And/Or/Not/BitAnd/BitOr/BitXor/ShiftL/ShiftR | unary minus (foldable — `Sub(0,x)` or a `Neg` node), assignment ops (shell arith has them via `Arith`) |
| `IrType {Int, Str, Any}` (A2 verdict) | the C type lattice + conversion rules |

The existing shell nodes are a *bonus*: `Output`/`WriteFile` map
printf/puts; the fd-table model maps `FILE*`; the store maps C globals;
`Arith` maps most integer expressions.

## 3. The type lattice (the #1 extension)

The A2 verdict becomes a real type system — C is unusable without it:

```
IrType ::= Void | Bool
         | I8 | U8 | I16 | U16 | I32 | U32 | I64 | U64   (signedness explicit)
         | F32 | F64
         | Char                                   (a byte, not a string)
         | Ptr(Box<IrType>)                        (subset — §5)
         | Array(Box<IrType>, usize)               (fixed-size; flexible at the end)
         | Struct(name) | Union(name) | Enum(name)
         | FnPtr(arg_types, ret)                   (subset)
```

Carried on: `Var`/`Decl`/`Function` params + returns, and on the
**conversion** step. The two hard parts:
- **Integer promotions / implicit conversions** — a defined lowering
  step, not ad-hoc (the IR must say when `char` widens to `int`).
- **Signed-overflow semantics** — the arith contract (A5a) already pins
  bash's 64-bit wrap; C's UB must be *defined away* by the subset
  contract (§6): the frontend accepts only wrapping semantics (or
  rejects signed overflow — the subset decides).

## 4. New nodes (sketch)

```
// expressions
IrExpr::Cast   { ty: IrType, expr: Box<IrExpr> }
IrExpr::Sizeof { ty: IrType }                     // folds at emit (target-dependent)
IrExpr::Member { obj: Box<IrExpr>, field: String, deref: bool }  // a.b / a->b
IrExpr::Comma  { l: Box<IrExpr>, r: Box<IrExpr> }
IrExpr::AddrOf { var: String }                    // &x — subset only
IrExpr::Deref  { ptr: Box<IrExpr> }               // *p — subset only

// statements
IrStmt::Label { name: String }
IrStmt::Goto  { label: String }
IrStmt::CSwitch { disc: IrExpr, cases: Vec<CSwitchCase> }  // fall-through explicit
struct CSwitchCase { values: Vec<u64>, body: Vec<IrStmt>, falls_through: bool }

// functions
IrStmt::Function { name, params: Vec<(String, IrType)>, ret: IrType,
                   storage: Storage, body: Vec<IrStmt> }    // static/extern/inline
```

The **goto problem** is the one with no universal backend answer:

| backend | goto |
|---|---|
| C, Perl, Lua (5.2+), Java (via labels — limited) | native/idiomatic |
| Zig, Rust | labeled loops only — a transform for general goto |
| JS | no goto — labeled break/continue where reducible + a state-machine transform otherwise |
| **Python** | **no goto** — state-machine transform mandatory |

The contract must therefore specify a **goto-reduction strategy** (the
structured-programming theorem applies: irreducible goto → a state
machine; reducible → loops/labels) so the IR can carry the label form
and each backend applies its transform — or the C frontend *rejects*
irreducible goto (a subset decision).

## 5. The pointer subset (the gate — FOR NOW)

Full C pointers → the dynamic backends need memory emulation (a heap of
objects + an address table) — heavy, slow, and semantically leaky. FOR
NOW the portable subset (the worker may later propose a
memory-emulation model that lifts it):

- **Arrays by (base, len)** — passed as pairs; no pointer arithmetic.
- **`&x` / `*p` only for pass-by-reference** — the call-by-reference
  idiom (`void swap(int *a, int *b)`), rendered per backend:
  | backend | pass-by-ref rendering |
  |---|---|
  | C/Zig/Rust | native pointers / `&mut` |
  | Java | arrays (the only ref-like) or a Box object |
  | JS | an object wrapper (or the arg array trick) |
  | Perl | `\` references |
  | Python | the `list`/mutable-arg idiom |
  | Lua | tables (the universal ref) |
- **No malloc/free** (lifetimes bounded by scope — the IR's static
  structure), no function pointers in data (FnPtr only for the
  direct-dispatch subset), no pointer arithmetic, no casts-through-void.

The subset contract (§6) is what makes the emulation matrix small.

## 6. The subset contract — FOR NOW (the C-side no-UB document)

**The boundary is provisional, not a permanent design decision.** The
ESTree worker owns the core; the contract below is the phase-1 envelope
that makes the frontend tractable, and the worker is free to think of
clever solutions that lift it (defined-UB semantics, a memory-emulation
model, richer preprocessor support, function-like macros — each judged
by the corpus + round-trip oracle, exactly the bash-fidelity-contract
discipline). FOR NOW the frontend accepts only **defined-semantics
portable C**:
- signed arithmetic wraps (the arith contract's wrapping — no UB);
- no uninitialized reads; no `setjmp`/`longjmp`; no varargs; no
  recursion beyond a (bounded, target-tunable) depth; no raw
  pointer arithmetic; no `union` type-punning;
- `#include` only for the stdlib surface the I/O bridge maps
  (stdio/string/stdlib/math — each mapped node-by-node to the IR);
- `#define`/`#ifdef` constants only (no function-like macros in the
  subset) — resolved by the preprocessor stage;
- I/O through the bridge: `printf/puts/fprintf` → `Output`/`WriteFile`;
  `scanf/getchar` → `read`; `FILE*` → the fd table.

Future directions the worker may pursue beyond the envelope (each an
independent plan entry when it gets there): pointer/memory emulation for
the dynamic backends, defined-UB arithmetic, full goto with a state
machine, function-like macros.

## 7. The architecture (frontend-only)

- **New frontend**: a C-subset parser (or a clang-style AST consumer)
  → the extended IR → the SAME ShIR JSON (A1) — the backends are
  **unchanged**.
- The preprocessor is a **frontend stage** (like the shell lexer): run
  the subset preprocessor (or accept preprocessed input) before parse.
- The shell-command nodes stay unused by the C frontend — the IR
  extension is one-directional and additive (existing backends ignore
  the new nodes; the structural gate + determinism verify).

## 8. The ask list

| # | Ask | Where |
|---|---|---|
| F1 | the type lattice (IrType extension) + the conversion step | `src/ir.rs` + `src/shir.rs` (the A2 verdict's generalization) |
| F2 | the new nodes: Cast/Sizeof/Member/Comma/AddrOf/Deref, Label/Goto/CSwitch, Function-signature/storage | `src/ir.rs` (additive; backends ignore) |
| F3 | the **goto-reduction contract** (reducible → labels; irreducible → state machine; or reject) | docs + the structural gate |
| F4 | the **pointer-subset contract** + the per-backend pass-by-ref matrix | docs |
| F5 | the **no-UB subset contract — FOR NOW** (wrapping, no UB, bounded recursion; the worker is free to lift it with cleverer solutions) | docs — the C-side sibling of the bash-fidelity contracts |
| F6 | the I/O bridge (printf/scanf/FILE* → Output/read/fd) | the frontend's lowering (no IR change) |
| F7 | the round-trip oracle: `C → --shir → C` corpus (identity test) | harness |
| F8 | the C-subset frontend itself (parser + lowering) | new crate/frontend |

Ordering: F1/F2 are the IR groundwork (additive, in the single-owner
window); F3–F5 are docs (doable now, like A4/A5/A7); F6/F7 are the
frontend proper; F8 is the frontend itself — the last, biggest step.

## 9. Honesty

- Easy: the imperative core maps (control flow, the store, fds, arith);
  the round-trip oracle gives the same discipline the corpus gives the
  shell path; the backends are untouched.
- Hard: the type lattice + conversions (the 80%); goto on the
  no-goto backends (Python/JS); pointer emulation on the dynamic
  backends — which is why the SUBSET is the phase-1 design — a FOR NOW
  boundary the worker may later lift with cleverer solutions.
- The value is proportional to the subset's size: a no-pointer,
  no-goto, wrapping-semantics portable subset covers the "string +
  integer + printf" programs that dominate real C-in-the-wild DSL
  output — and those map to the shell-family targets *beautifully*
  (they're the same programs the shell IR already handles).

## 10. Relationship to the other docs

The frontend mirror of `backend-c-core-needs.md`: the C backend asks
"A1–A7" (what core must publish); this asks "F1–F8" (what core must
grow). The universal contract (`backend-universal-contract.md`) is the
shared spine — both C-as-backend and C-as-frontend consume the same
serialized ShIR + annotations; the frontend simply adds the type/memory/
control surface. When the C backend lands, C→ShIR→C is the identity
test that validates BOTH.
