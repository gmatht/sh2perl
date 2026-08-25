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
