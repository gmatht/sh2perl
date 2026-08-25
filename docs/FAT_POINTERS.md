# Fat Pointers vs Fallback Pointers vs InvisiCaps

## The three models compared

### Our current fallback pointers

Every unclassifiable variable becomes:

```c
char *x = NULL;          // no len, no cap, no ownership info
x = strdup(_cap_0());    // leak on reassign, overflow if source too long
printf("%s", x);         // strlen(x) every time, crash if x is garbage
```

Properties: zero overhead, zero safety, zero introspection. The C
program cannot ask "how long is x?" without strlen(), cannot know if
x owns its memory or aliases another buffer, and cannot detect
use-after-free or double-free.

### Fil-C's InvisiCaps

Every pointer carries invisible metadata (lower bound, upper bound,
aux word) stored OUTSIDE the program's address space:

```
Program sees: intval (the raw 64-bit address)
Runtime tracks: lower bound, upper bound, aux (in shadow memory)
```

Properties: complete memory safety (OOB trap, UAF panic), 64-bit ABI
compatible, ~4× overhead. Requires custom compiler (FIL), custom
allocator (GC-based isoheap), and runtime library.

### Fat pointers (visible metadata)

The metadata lives IN the struct the program can inspect:

```c
typedef struct {
    char *ptr;
    size_t len;
    size_t cap;
} sh2_str;
```

Properties: bounds known at runtime, capacity tracked (safe growth),
ownership explicit. No invisible machinery — the code generator emits
every access. Overhead proportional to number of accesses, not a fixed
multiplier.

## Why we don't need InvisiCaps

Fil-C must handle arbitrary C code written by humans who do evil things:
union type punning, cast function pointers to void*, mmap and alias,
race threads on shared pointers. Its capability system must survive ALL
of that while remaining transparent.

We generate every line of C from a typed IR. There are no unions, no
function pointer casts, no thread races (the generated code is single-
threaded except for background jobs which are separate processes), no
mmap, no type punning. Every variable declaration comes from our own
emit_var_decl. Every assignment comes from our own Assign arm.

This means we can use VISIBLE fat pointers — structs the code generator
emits and fills — instead of INVISIBLE capabilities stored in shadow
memory. The safety guarantee is weaker (no protection against our own
codegen bugs) but the performance cost is near zero for well-typed vars.

## When fat pointers help

The gap in our storage-class matrix is between:

1. **Inline buffer** (`char x[1024]`) — fast but requires proven bounds;
   truncates silently when the proof is wrong
2. **Heap strdup** (`char *x`) — handles any length but no bounds info,
   leaks on reassignment, crashes on overflow

Fat pointers fill the middle ground: UNBOUNDED length but LOCAL scope.
Currently these fall back to raw `char*` + strdup which is dangerous.

Concrete examples where a fat pointer beats both alternatives:

| Construct | Inline buf fails because | Raw char* fails because |
|---|---|---|
| `x=$(find / -name '*.c')` | output can exceed any bound | no length tracking, leak on reassign |
| `mapfile -t arr < large_file` | each line unknown length | same |
| `x=$(sha256sum f)` | 64+filename chars, under-bounded | same |
| `read -r line` in a loop | lines vary wildly | same |

With fat pointers these become:
```c
sh2_str x = {0};
sh2_str_assign(&x, _cap_0());   // grows as needed, tracks len
```

## When fat pointers hurt

1. Numeric vars (`long long x`) — tagged/fat wastes 24+ bytes per var
2. Proven-bounded vars already using inline buffers — adding metadata
   doubles the footprint for no gain
3. Hot inner loops scanning characters — extra indirection through
   `.ptr` on every iteration hurts cache locality
4. Simple scripts with < 10 variables — the infrastructure outweighs
   the benefit

## Implementation sketch

### Minimal viable fat pointer

```c
typedef struct {
    char  *ptr;
    size_t len;    // strlen(ptr), maintained incrementally
    size_t cap;    // allocated bytes (>= len+1)
} sh2_str;

static void sh2_str_reserve(sh2_str *s, size_t need) {
    if (!s->ptr || s->cap < need) {
        size_t new_cap = s->cap ? s->cap * 2 : 64;
        while (new_cap < need) new_cap *= 2;
        s->ptr = realloc(s->ptr, new_cap);
        s->cap = new_cap;
    }
}

static void sh2_str_set(sh2_str *s, const char *v) {
    size_t n = strlen(v);
    sh2_str_reserve(s, n + 1);
    memcpy(s->ptr, v, n + 1);
    s->len = n;
}
```

### Integration into emit_var_decl

Replace the three-way branch:
```c
if (is_num(v))     → long long x = 0;
else if (bounded)  → char x[N+1] = "";
else               → char *x = NULL;
```
with a four-way:
```c
if (is_num(v))              → long long x = 0;
else if (bounded ≤ 256)     → char x[N+1] = "";       // inline stays
else if (escape == Local)   → sh2_str x = {0};        // NEW: fat ptr
else                        → char *x = NULL;          // escaping: raw
```

Note the threshold change: inline buffers stay for SHORT bounded vars
(most shell vars!), fat pointers take over for LONG or unbounded ones,
raw pointers remain only for escaping vars.

### Assignment lowering

Current: `{id} = strdup({v});` or guarded copy
Fat: `sh2_str_set(&{id}, {v});`

### Read lowering

Current: `(char*)({id})`
Fat: `{id}.ptr` (with `{id}.len` available for free)

### Concat/append

Current: `_sh_add` into a command buffer then capture
Fat: `sh2_str_append(&{id}, {v});` — amortized O(1)

## Relationship to the existing storage classes

| Storage class | Can hold fat ptr? | Notes |
|---|---|---|
| Immediate numeric | N/A | no string involved |
| Const-lifted literal | No | read-only, compile-time known |
| Inline buffer | No (it IS the storage) | but could return .data slice |
| Heap strdup | YES — natural replacement | same lifetime model |
| Arena string | Partially — arena manages memory | fat ptr adds len/cap tracking |

## When to use each (revised decision tree)

```
Int-proven? ─YES→ immediate numeric
     │NO
Bound-known AND ≤ MAX_STACK_ALLOC AND local? ─YES→ inline buffer
     │NO
Type-stable across paths? ─YES→ keep whatever it currently is
     │NO (mixed/unpredictable)
Unbounded AND local scope? ─YES→ FAT POINTER (sh2_str)
     │NO (escapes function)
Raw heap pointer (current behavior)
```

The fat pointer occupies the sweet spot: more safety than raw char*
without requiring compile-time proofs, less overhead than full
capability tracking.
