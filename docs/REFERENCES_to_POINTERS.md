# References → Pointers: storage-class selection for the C backend

## The problem

Bash has no pointer types. A variable is a name bound to a value; the
runtime store is a hash map from names to strings (with optional integer
attributes). When we translate to C, every variable reference must become
a concrete C storage class — and the choice determines correctness,
performance, and memory safety.

The C backend currently uses three classes, chosen by ad-hoc checks
scattered across `emit_var_decl`, `buf_bound`, and the Assign arm:

| Current class | C decl | Chosen when |
|---|---|---|
| Const-lifted literal | `const char x[N] = "…"` | single assign, literal RHS |
| Fixed buffer | `char x[N+1]` | length provably ≤ N ≤ 1024 |
| Heap pointer | `char *x = NULL` + strdup | everything else |
| Numeric | `long long x` / `int x` | range analysis proves Int |

This works but leaves performance on the table and occasionally produces
unsound code (a const-lifted var that is later mutated by an in-place
split segfaults). This document defines the full decision matrix.

## The five storage representations

### 1. Immediate value (no pointer at all)

```c
long long x = 42;
unsigned int port = 8080;
```

**When**: the range/provenance analysis (`analyze_var_ranges`,
`var_widths`) proves the var holds only integers within a known width.
Already implemented — numeric vars bypass string machinery entirely.

**Detection**: `is_num(name) == true` (from `var_types`), which the
range analysis populates from `ArithAst::Assign` / `IncDec` targets.

### 2. Inline buffer (stack-allocated array)

```c
char name[24];   // fits in the SSO threshold
char path[1024]; // bounded by var_lengths analysis
```

**When**: ALL of:
- length provably ≤ MAX_STACK_ALLOC (configurable, default 1024,
  `FIXED_BUF_CAP` in c_backend.rs)
- the variable may be PASSED to sub-functions freely — passing by
  pointer is safe as long as callees do not STORE it anywhere that
  outlives the allocating frame (standard borrow discipline: read-only
  access in callees is always fine)
- no callee or code path STORES the pointer beyond the call (no global
  arrays, no heap structures retaining it, no return of the pointer)
- var is not const-lifted (would need `const char[]` instead)

**What disqualifies**: only ESCAPE through storage. Passing to a
function that reads the buffer and returns is fine. Storing it in a
global array, returning it from the function, or saving it in a
longer-lived data structure is escaping.

**Advantage**: zero malloc, zero free, cache-friendly, dies with scope.
The existing `buf_bound()` already computes this for N ≤ MAX_STACK_ALLOC.

**Configurable threshold**: `FIXED_BUF_CAP` controls the maximum stack
allocation size. Above this, even otherwise-perfect candidates go on
the heap — large buffers risk stack overflow in deeply recursive
generated code. Default: 1024 bytes.

**Detection gap**: `var_lengths` under-bounds command output
(sha256sum = 64 chars + filename). The fix is NOT to raise bounds but
to check escape class first: if non-escaping AND all assignments have
literal or bounded RHS → stack buffer. If ANY assignment is a capture
→ heap (unless the capture result is provably short).

### 3. Arena-allocated string

```c
// arena: bump allocator, bulk-freed at scope exit
char *s = sh2_arena_strdup(&arena, val);
```

**When**: MANY short-lived strings in the same scope (loop bodies,
command output captures). No individual frees needed. The entire arena
is freed once when the enclosing function returns.

**Advantage over strdup-per-assignment**: no fragmentation, better
cache locality, O(1) dealloc (pointer reset).

**Detection**: a function body containing ≥ K capture sites (K ≈ 3)
whose results are consumed as strings but not stored in global/exported
vars. The capture results live only within the iteration.

**Current gap**: the backend does `strdup(_cap_N())` per call — each
capture allocates and the old value leaks (or requires explicit free).
An arena eliminates both problems.

### 4. Reference-counted (RC) pointer

```c
typedef struct { char *data; int refcnt; } sh2_rc_str;
sh2_rc_str *p = sh2_rc_new("value");
sh2_rc_ref(p);   // alias taken
sh2_rc_unref(p); // alias dropped
```

**When**: the var is ALIASED — multiple names reference the same
storage with unknown relative lifetimes. In bash this happens via:
- `declare -n ref=target` (namerefs)
- `local -n` in functions
- Indirect expansion `${!indirect}`

**Detection**: nameref declarations (parseable from Declare stmts with
the `-n` attribute), plus escape_classes verdict == `Store` where the
store key appears as BOTH source and target of indirect accesses.

**Reality check**: bash namerefs are rare in practice. The current
backend doesn't support them (no `declare -n` handling exists).
RC pointers are the correct target representation IF/WHEN namerefs are
added, but the cost/benefit doesn't justify implementing them preemptively.

### 5. Tagged value (the unified scalar)

```c
typedef struct {
    enum { SH2_STR, SH2_NUM, SH2_NULL } tag;
    union {
        char *heap;           // heap/arena allocated
        char sso[23];         // small-string optimization (inline)
        long long num;
    };
} sh2_val;
```

**When**: the type of a variable is UNKNOWN at compile time — it might
hold "hello" or 42 depending on runtime path. Currently rendered as
`char*` with atoll coercion, losing type safety and wasting allocation
on numeric values.

**The tagged approach**: PHP's zval, Perl's SV, JS's NaN-boxing. One
representation covers all cases without per-access coercion.

**Advantage**:
- No `atoll(getenv(...))` chains — direct `.num` access
- SSO avoids malloc for strings ≤ 22 bytes (most shell vars!)
- Type transitions tracked at runtime (bash semantics preserved)

**Cost**: 32 bytes per var vs 8 for a bare pointer. Cache pressure if
every var becomes a tagged value.

## Decision matrix

```
                    Int-proven?  Bound-known?  Escapes?  Aliased?
                        │            │            │          │
              YES ──→ immediate       │            │          │
               NO ──→ ┌───────────────┤            │          │
                      │               │            │          │
                YES ──→ inline buf    │            │          │
                 NO ──→ ┌─────────────┤│            │          │
                        │             ││            │          │
                  YES ──→ heap ptr    ││            │          │
                   NO ──→ ┌───────────┤││            │          │
                          │  arena if │││            │          │
                          │  hot loop │││            │          │
                          │  else     │││            │          │
                          │  heap ptr │││            │          │
                          └───────────┘┘│            │          │
                                        │            │          │
                     nameref? ──YES──→ RC pointer   │          │
                      NO ──────────────────────────→ tagged value
```

Simplified decision tree (in evaluation order):

1. **Int-proven** → immediate numeric (already done)
2. **Nameref** → RC pointer (future work)
3. **Type-unknown across paths** → tagged value (proposed)
4. **Bound-known AND local AND non-capture-RHS** → inline buffer (partially done)
5. **Hot-loop capture consumer** → arena (proposed)
6. **Default** → heap pointer with strdup (current behavior)

## Detection: what analyses feed each decision

| Decision input | Source analysis | Status |
|---|---|---|
| Int-proven | `analyze_var_ranges` + `var_widths` | ✅ done |
| Bound-known | `analyze_string_lengths` → `var_lengths` | ✅ done (under-bounds for captures) |
| Local vs Store | `escape_classes` transform | ⚠️ exists, C renderer ignores it |
| Const-marked | `analyze_var_const` → `var_const` | ✅ done (but conflicts with in-place split) |
| Capture-target | `collect_capture_vars` → `capture_vars` | ✅ done |
| Nameref | parser (not yet parsed) | ❌ future |
| Type-unknown | NEW: track whether a var is assigned both Str and Num across paths | ❌ proposed |

## The construct-normalisation connection

The sibling agent's normalisation merge changed IR shapes (e.g., `read
VAR` → `Assign{VAR, Ext(ReadLine)}`). These new shapes affect storage
class decisions:

- `Ext(ReadLine)` assigned to VAR → VAR is unbounded (line length unknown)
  → heap pointer or arena, never inline buffer
- `let-cond` → native Arith comparison → numeric vars stay numeric
- `WordCount{text}` → result is always Int → immediate numeric

Each new normalisation should declare its OUTPUT TYPE so the storage
selector can pick correctly. The `.node` format can carry this:

```
node ReadLine
tag "ReadLine"
kind expr
output_type: str_unbounded   ← new: informs the storage selector
```

Without this metadata, every backend guesses (and guesses wrong).

## Implementation roadmap

### Phase 1 — wire up escape_classes (highest value, lowest risk)
The C renderer currently ignores `escape_classes::verdict()`. Wiring it
means:
- `Local` vars → prefer inline buffer over heap even when buf_bound is
  marginal (e.g., bound 1024 → stack instead of heap for locals)
- `Store` vars → keep heap pointer, add export/import sync points
This alone could eliminate most `strdup` calls for simple scripts.

### Phase 2 — capture-var arena (medium value)
Replace per-call `strdup(_cap_N())` with arena allocation inside
functions. The arena is created at function entry, freed at return.
Capture results point into the arena. No individual frees.

```c
sh2_arena _fn_arena = {0};  // at fn entry
echo_result = sh2_arena_strdup(&_fn_arena, _cap_0());
// … no frees …
// at return: compiler inserts sh2_arena_free(&_fn_arena);
```

### Phase 3 — tagged values for type-unknown vars (high value, high risk)
Only for vars whose escape class is `Store` AND whose assignments mix
numeric and string expressions. The tagged representation preserves
correct semantics while eliminating atoll/strdup coercion chains.
Requires changing every access site — large diff, needs careful testing.

### Phase 4 — namerefs via RC (deferred until parser support lands)
`declare -n` parsing → nameref detection → RC pointer emission.
Blocked on parser support for `declare -n`.

## Relationship to the split-in-place pass

The in-place tokenizer writes NUL bytes into the source buffer,
destroying it. This is ONLY safe when the source is:
1. NOT const-lifted (`const_lifted.contains(xv) == false`)
2. NOT shared with another live variable (no aliasing)
3. Dead after the loop (liveness proof)

These conditions map to storage classes:
- Condition 1 fails → the var was const-lifted because it's
  single-assign + literal → it SHOULD be immutable by the escape
  analysis → the in-place optimisation is inappropriate for it
- Condition 2 passes → tokens alias the buffer ✓
- Condition 3 is the liveness precondition itself

So the correct integration: the split-inplace transform should SKIP
vars whose escape class is `Local` AND whose only assignment is a
literal (because those get const-lifted). Instead, it should target
vars with capture-assigned values (`Store` class, mutable strdup'd
storage) — those benefit most from eliminating the copy.

## Summary

The five storage representations form a spectrum from fastest/most
constrained to slowest/most flexible. The analyses exist to classify
every variable into the right bucket. What's missing is wiring them
together into a single storage-selector function that emit_var_decl
consults, replacing the current ad-hoc checks.

| Representation | Speed | Safety | Complexity | Priority |
|---|---|---|---|---|
| Immediate numeric | ★★★★★ | ★★★★★ | Done | — |
| Inline buffer (stack, ≤ MAX_STACK_ALLOC) | ★★★★☆ | ★★★★☆ | Partially done | Phase 1 |
| Arena strings | ★★★★☆ | ★★★☆☆ | New | Phase 2 |
| Tagged value | ★★★☆☆ | ★★★★☆ | Large diff | Phase 3 |
| RC pointer | ★★★☆☆ | ★★★★★ | Deferred | Phase 4 |
| Heap strdup | ★★☆☆☆ | ★★★☆☆ | Current default | Baseline |

## The sh2_str integration path (C backend)

Switching from raw `char *x = NULL` to `sh2_str` touches every site that
reads or writes an unbounded variable. There are six categories:

| Access-site category | Count | Current C form | sh2_str form |
|---|---|---|---|
| `store_ref(name)` read | 19 | `(x ? x : "")` | `sh2_str_get(&x)` |
| Format-string cast | 15 | `(char*)(x)` | `sh2_str_get(&x)` |
| strdup assignment | 14 | `x = strdup(v);` | `sh2_str_set(&x, v);` |
| `_sh_export` to child | 17 | `_sh_export(n, x);` | `_sh_export(n, sh2_str_get(&x));` |
| Guarded copy (bounded) | 8 | unchanged (inline stays) | unchanged |
| `getVar` dispatch arms | 4 | mixed reads/writes | per-arm update |

Total: ~66 sites across c_backend.rs. All mechanical replacements; no
logic changes required. The diff is large but each hunk is independent.

### What does NOT change
- Numeric vars (`long long`) stay as-is
- Bounded inline buffers (`char x[1024]`) stay as-is
- Capture helpers (`_cap_N()`) still return `char *`
- Site command text (`_sh_badd`) stays as-is (child bash reads via env)

## How much is cross-backend?

The division is strict:

**CROSS-BACKEND (shared analyses + IR vocabulary):**

| Component | Where it lives | Every backend uses? |
|---|---|---|
| getVar / setVar node family | core IR | ✅ all |
| escape_classes verdicts | transforms/escape_classes.rs | ✅ all (when wired) |
| var_lengths bounds analysis | shir.rs analyze_string_lengths | ✅ all |
| analyze_var_const Const/Var verdicts | shir.rs | ✅ all |
| capture_vars set | c_backend.rs collect_capture_vars | ⚠️ C only (should be shared) |
| WordCount/Split/StrLen/etc nodes | shir_nodes/*.node | ✅ all (via Ext dispatch) |
| Storage-class decision matrix | REFERENCES_to_POINTERS.md (design) | concept shared, impl per-backend |

**PER-BACKEND (representation + runtime helpers):**

| Concept | C implementation | JS/Estree equivalent | Perl equivalent | Go equivalent |
|---|---|---|---|---|
| Managed string | `sh2_str { ptr }` | `sh2.vars.x` (plain property) | `$x` (scalar SV) | `var y string` |
| Bounds checking | compile-time only | V8 runtime (free) | runtime (free) | compiler + runtime |
| Use-after-free detect | ❌ none | GC handles it | refcounting | compiler |
| Type coercion | `atoll(x)` explicit | implicit (dynamic) | implicit (scalar context) | explicit |
| Arena allocation | custom bump allocator | GC young gen | arena allocator | GC |
| In-place mutation | `char buf[]` writable | ❌ immutable strings | ✅ mutable | ✅ mutable []byte |
| RC pointer | custom refcnt | GC refcount (V8 internal) | SV refcount | GC |

**Key insight**: the ANALYSES that decide which representation a variable
gets are always cross-backend. The REPRESENTATIONS themselves are
per-backend because each language has different native capabilities.
The shIR doesn't need new node types for any of this — it needs the
ANALYSES to publish their verdicts so every backend can consult them.

What IS missing at the shIR level: a way for a transform to DECLARE "this
variable should be stored as X" without knowing what X means in each
backend. The `.node` format could carry an `output_type` hint:

```
node ReadLine
tag "ReadLine"
kind expr
field text: expr
output_type str_unbounded   ← informs every backend's storage selector
```

Each backend maps `str_unbounded` to its own representation:
- C → `sh2_str` (or raw `char*` if escaping)
- JS → plain string (already correct)
- Perl → scalar (already correct)
- Go → `string` (already correct)

This is the missing piece that makes storage-class selection truly
cross-backend without coupling backends.

## Integration effort estimate

| Phase | Sites touched | Risk | Value |
|---|---|---|---|
| sh2_str runtime preamble | 1 (emit_runtime) | zero | infrastructure |
| store_ref → sh2_str_get | ~19 call sites | low | correctness (no NULL deref) |
| strdup → sh2_str_set | ~14 assignment arms | low | no leaks on reassign |
| export → sh2_str_get | ~17 child-bash exports | low | consistency |
| format casts → sh2_str_get | ~15 printf args | low | no crash on garbage |
| guarded copies → skip for sh2_str | ~8 sites | medium | avoids double-buffer |
| **Total** | **~66 mechanical replacements** | | |

All are one-line substitutions with no control-flow changes. The diff
is large but each hunk is trivially reviewable.

## Multi-frontend considerations

### The frontend fleet

The A1 contract is language-neutral: any frontend in any language emits
the same ShIR JSON. The current fleet includes:

| Frontend | Source lang | Testdata | Key constructs |
|---|---|---|---|
| bash (core) | shell | 551 corpus | everything |
| c-sh-go | C subset | 105 files | AddressOf, Deref, asm, typed decls |
| cpp-sh-go | C++ superset | shared w/ C + extras | classes, references |
| posix-sh-go | POSIX shell | 88 files | no arrays, no [[ ]] |
| go-sh | Go subset | 4 files | typed vars, GC'd strings |
| zsh-sh-go | ZSH subset | typeset, assoc arrays | |
| fish-sh-go | Fish subset | no positional params | |
| rust-frontend | Rust subset | syn-based, ownership-aware | |
| py-sh-go | Python subset | dynamic typing | |
| powershell-sh-go | PowerShell subset | pipeline objects | |
| perl-sh-go | Perl subset | scalar/array/hash sigils | |
| bat-sh-go | Batch/CMD | labels + goto | |

Each frontend's constructs map to different storage-class requirements.
The same `getVar("x")` node can mean:

- **bash**: read from the runtime store (could be anything)
- **C frontend**: dereference a typed pointer (`int *p` or `char *s`)
- **Rust frontend**: move or borrow a value (ownership semantics apply)
- **Go frontend**: read an interface{} (always heap + type tag)
- **Python frontend**: read an attribute from a dict (always tagged)

### How the frontend affects storage-class selection

The storage-class decision matrix in this document was designed for
bash-to-C translation. Other frontends produce IR shapes that need
DIFFERENT representations:

#### C frontend → C backend (identity mapping)

The C frontend emits `AddressOf{operand}` and `Deref{pointer}` for
`&x` and `*p`. In the C backend these are IDENTITY operations:
- `AddressOf{x}` → `&x` (a real C pointer)
- `Deref{p}` → `*p` (a real C dereference)

These MUST NOT be routed through managed strings or arenas. The C
frontend's variables already have correct C storage classes chosen by
the C compiler's own semantics.

```
Frontend: int *p = malloc(16); *p = 42;
shIR:     Assign{p, Ext(AddressOf{...})}
          Expr(Ext(Deref{pointer: Var("p")}))
C render: int *p = malloc(16); *p = 42;
```

#### Rust frontend → C backend (ownership → raw pointer)

Rust has ownership/borrowing. The frontend tracks lifetimes; the C
backend doesn't need to. Every Rust variable becomes either:
- `char *x` / `long long x` (if owned and escaping)
- stack buffer (if owned and non-escaping and bounded)

Rust's borrow checker guarantees no use-after-free, so the C backend
doesn't need RC or arena — just plain values/pointers.

#### Go frontend → C backend (GC → explicit)

Go strings are immutable + GC-managed. The Go frontend emits captures
and string operations as sh2.* calls. The C backend maps them to
either arena strings (hot loops) or strdup (cold paths).

#### Python frontend → C backend (dynamic → static)

Python is dynamically typed like bash. All Python variables become
tagged values or abstract refs — the type is never known at compile
time. This is the worst case for the C backend.

#### Batch frontend → C backend (no types at all)

Batch (.bat) has no data types, no expressions, only string
substitution. Everything is a `char[]`. No numeric vars, no arrays,
no pointers needed.

### Storage-class hints in .node declarations

Ext nodes declare their output characteristics so every backend can
select the right storage without understanding the construct's semantics:

```
node WordCount
tag "WordCount"
kind expr
field text: expr
output_type integer        ← always produces a number
```

```
node ReadLine
tag "ReadLine"
kind expr
output_type str_unbounded  ← unknown length, needs managed buffer
```

```
node PathName
tag "PathName"
kind expr
field path: expr
output_type str_bounded    ← substring of input, bounded by input length
```

Backend mapping of output_type:

| output_type | C rendering | JS rendering | Notes |
|---|---|---|---|
| `integer` | `long long` | number | no allocation |
| `str_bounded(N)` | `char[N+1]` | string | stack if N ≤ MAX_STACK_ALLOC |
| `str_unbounded` | `sh2_str` or `char*` | string | fat ptr if local, raw if escapes |
| `ref_mutable` | `T *` | N/A | only C-like backends |
| `ref_readonly` | `const T *` | const reference | borrowed |

Without this metadata, each backend must infer the output type from
the construct's semantics — which is fragile when new frontends emit
shapes no backend has seen before.

### Frontend-specific escape semantics

Escape analysis must account for frontend-specific lifetime rules:

| Frontend | String mutability | Lifetime model | Arena safe? |
|---|---|---|---|
| bash | mutable (in-place split OK) | scope-bounded | usually yes |
| C | mutable (by design) | manual | yes if no escape |
| Go | immutable | IR-provable (lifetime known) | usually unnecessary — most vars become InlineBuffer or ManagedString |
| Rust | depends on type | ownership-checked | unnecessary |
| Zsh | mutable like bash | scope-bounded | usually yes |
| Fish | immutable lists | GC | unnecessary |
| PowerShell | objects, mutable properties | IR-provable for scalars | usually unnecessary |

For backends targeting GC'd languages (JS, Go), arena allocation and
in-place mutation are unnecessary complexity — the runtime handles it.
Only C, C++, Rust, and Zig backends benefit from these optimisations.

### The cross-backend contract

The A1 schema is the ONLY interface between frontends and backends.
Storage-class selection is entirely a BACKEND decision informed by:

1. The shIR shape itself (what nodes appear)
2. The `.node` output_type hints (what the construct produces)
3. Backend-local analyses (var_lengths, escape_classes, var_const)

No frontend needs to know how the C backend stores its variables.
No backend needs to know why the C frontend emitted AddressOf instead
of a Capture. The A1 contract is sufficient.

This means: adding a new frontend NEVER requires changing existing
backends. Adding a new backend requires implementing the primitives
it will support (or refusing those it won't).

## Why global buffers are NOT the default: the uniqueness requirement

### The problem

A global/static buffer seems attractive for unproven-escape pointers:
no leak (reused), no dangling pointer (never freed). But using one
requires proving **uniqueness**: at every point during execution,
exactly ONE live variable references each static buffer.

Without this proof, overwriting the buffer corrupts every other
variable that still points into it. Concrete example from the corpus
(alias.sh):

```bash
f() { local msg="inner"; echo "$msg"; }
g() { local msg="outer"; f; echo "$msg"; }
g()
# bash: inner / outer  ✓ (local creates per-function scope)
# C:    inner / inner  ✗ (both share the same global char *msg)
```

Both functions' `local msg` assignments target the same C global
because the renderer hoists `msg` to file scope. Without proper
per-function storage isolation, any global buffer strategy produces
silent data corruption.

### What uniqueness requires proving

For each global buffer B, at every program point P:
1. No OTHER variable holds a pointer into B
2. No function save/restore has created an active alias into B
3. No array element or struct field references into B
4. If B is inside a recursive call chain, all outer frames'
   references are dead

Conditions 1–3 require full alias analysis across all code paths.
Condition 4 requires interprocedural call-graph analysis. Together
they are equivalent to Rust's borrow checker — nontrivial to implement
correctly for a generated-code system.

### Why heap allocation IS the right default

| Property | Heap strdup | Global static buffer |
|---|---|---|
| Uniqueness | Trivially yes (fresh alloc each time) | Requires proof |
| Aliasing | Impossible between distinct vars | Possible via pointer copy |
| Recursion safety | Each frame gets its own copy | Shared buffer corrupts |
| Thread safety | Safe (different allocations) | Unsafe without locks |
| Leak on scope exit | Yes (must free or leak) | N/A (reused) |

Heap strdup's only downside is potential memory growth. For shell
scripts (bounded execution, small data), this is acceptable — the
process exits before memory becomes a problem.

### When global buffers DO work safely

1. **Per-capture-site buffers** (`_cap_N()`): each site has its own
   static buffer, no other site writes to it. The caller must consume
   the result before calling again. Safe because:
   - Each capture has a dedicated buffer (no cross-site aliasing)
   - The caller immediately consumes (strdup or print)
   - No recursion through capture paths in the corpus

2. **Command-text buffers** (`_sh_cmd`): built once per site, consumed
   by popen/system, then reset. Sequential usage guarantees no overlap.

3. **Wrap buffers** (`_sh_wrap`): same pattern — built, used by system,
   done. Single-threaded sequential access.

These work because the USAGE PATTERN guarantees uniqueness: build →
use → discard, never two live references simultaneously. The pattern
is enforced by construction (the code generator emits them in this
order), not by analysis.

### Caller-allocates as an alternative

When the CALLER can bound the result size, it can allocate the buffer
and pass it to the callee — eliminating the ownership question entirely
(Win32 API pattern):

```c
// caller allocates, callee fills
char buf[256];
gethostname(buf, sizeof(buf));       // caller owns, callee fills

// vs callee-allocates (current):
char *result = _cap_0();             // callee allocates static buf
// or:
char *result = strdup(_cap_0());     // callee allocates heap (leak-prone)
```

For shell-to-C, caller-allocation works when the result size is
provably bounded (basename/dirname of a known-length input, numeric
formatting, substring extraction). It does NOT work for command output
(unbounded).

### Summary

| Strategy | Uniqueness proof needed? | Safe without proof? | Best for |
|---|---|---|---|
| Heap strdup per assignment | No | Yes | Default for unproven cases |
| Per-site static buffer | Pattern-enforced (build→use→discard) | Yes within pattern | Capture results |
| Shared global buffer | YES — full alias + liveness analysis | No | Avoid until analysis exists |
| Caller-allocated buffer | Caller controls lifetime | Yes if bounded | Provably-bounded results |

## Trust boundary: source language determines optimisation safety

### The core insight

Storage-class selection AND optimisation safety depend on which FRONTEND
produced the IR — not just on the IR shape itself. The same Ext(Split)
node has different safety implications depending on whether a bash or C
frontend emitted it:

| Guarantee | sh/bash/zsh | C/C++ | Rust | Go | Python |
|---|---|---|---|---|---|
| Copy-on-assign at SOURCE level | ✅ guaranteed | ❌ pointers alias | per-type | strings immutable | strings immutable |
| In-place mutation safe at SOURCE level? | if liveness-proven | needs alias analysis | borrow checker | N/A for source strings | N/A |
| **After shIR analysis: uniqueness provable?** | **YES** | **usually yes** (most vars don't alias) | **YES** | **YES** (IR tracks lifetime) | **YES** (IR tracks lifetime) |

**The compiler advantage**: the SOURCE language needs GC/runtime tracking
because it cannot analyse the program at parse time. The shIR CAN — it
sees every assignment, every read, and every scope boundary. A variable
that a Python runtime must GC-track because its type is unknown at
parse time has a KNOWN type in our IR (from usage patterns). The GC
was compensating for missing compile-time information that we now have.

For sh/bash frontends: every assignment does strdup/copy. Two variables
NEVER share the same buffer. So in-place mutation of one variable cannot
affect another — the only precondition is liveness (the var is dead after).

For C/C++ frontends: `char *a = b` creates an ALIAS, not a copy.
Mutating through `a` changes what `b` sees. In-place mutation requires
proving that NO other pointer aliases the buffer — which requires
interprocedural alias analysis that we don't have.

### Implementation: frontend trust level

Each frontend should declare a trust level that the storage selector
consults:

```rust
pub enum FrontendTrust {
    /// Source language guarantees copy-on-assign (bash family).
    /// In-place mutation safe when liveness-proven.
    CopyOnAssign,
    /// Source language allows pointer aliasing (C family).
    /// In-place mutation requires explicit non-alias proof.
    PointerAliasing,
    /// Source language has GC / immutable strings (Go, JS, Python).
    /// No in-place mutation needed; GC handles lifetime.
    Managed,
    /// Source language tracks ownership (Rust).
    /// Borrow checker verdicts are authoritative.
    OwnershipTracked,
}
```

Stored as a field on IrProgram:

```rust
pub frontend_trust: FrontendTrust,
```

Populated by each frontend when it emits A1 JSON:

```json
{"type":"Program", "frontend_trust": "CopyOnAssign", ...}
```

### Effect on optimisation passes

| Pass | CopyOnAssign | PointerAliasing | Managed |
|---|---|---|---|
| split-inplace | fire when liveness-proven | NEVER (aliasing risk) | skip (no mutable bufs) |
| const-lift exclusion for split targets | needed | not needed (already conservative) | not needed |
| arena allocation | safe if all non-escaping | needs full graph analysis | unnecessary (GC) |
| managed string (sh2_str) | for unbounded locals | for unbounded locals | unnecessary |

### What this means for the current implementation

The split-inplace pass and managed-string wiring are currently SAFE
because only the bash frontend produces these IR shapes. When the C
and C++ frontends mature, they must either:
1. Declare `PointerAliasing` trust → the pass skips their statements
2. Or provide their own non-alias proofs per-variable

Without this trust boundary, enabling the transform globally would be
UNSOUND for C-frontend scripts that use pointer aliasing.

### The compiler advantage: eliminating runtime machinery through analysis

The source language needs GC/runtime tracking because the RUNTIME cannot
analyse the program — it discovers types and lifetimes during execution.
Our shIR pipeline sees the ENTIRE program at translation time:

| Runtime mechanism in source lang | Why the source needs it | Why our IR doesn't |
|---|---|---|
| Python `gc.collect()` | Types unknown at parse time | `var_types` + usage patterns determine type per var |
| Go `runtime.GC()` | Escape analysis too expensive at compile time | `var_lifetimes.escapes` proves scope-boundedness per var |
| JS garbage collector | Closures create unbounded lifetimes | `escape_classes` classifies each capture as Local/Store |
| Perl SV refcounting | Dynamic sigils defeat static typing | Each var has ONE sigil, known from declaration |

**The translator's advantage**: we see the whole program. The source
runtime only sees one variable at a time. Every "the compiler can't
know this" in the source language becomes "we computed this" in the IR.

This means: for MOST variables from ANY frontend, the storage-class
selector should find InlineBuffer or ManagedString — NOT "needs GC."
The GC was compensating for missing compile-time information that our
IR now provides. Only genuinely dynamic constructs (eval, indirect
access through data-driven names) require runtime-managed storage.

**Practical implication**: the default storage class should be
InlineBuffer or ManagedString for ALL frontends, not just bash.
The "unnecessary (GC)" classification was premature — it gave up on
analysis that our IR actually supports.
