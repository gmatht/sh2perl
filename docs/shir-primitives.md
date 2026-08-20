# ShIR Primitives — the core decides, backends implement

## Principle

A shIR node that represents a **shell command's result** should NOT be a
bespoke node that every backend must implement with command-specific logic.
Instead, the **core** (the transform/lowering pass) reduces each shell
command to a **composition of a small set of universal, language-neutral
primitives**. Each backend implements each primitive once, trivially; the
**core owns the decision** of which composition (and which primitives) a
command becomes.

> "The core should be able to reduce the shIR to nodes that are natural to
> the target language. If that needs a regex, that isn't something that
> every backend should have to decide for itself."

So the division of labor is strict:
- **The core decides** what `wc -l` means, how `cut` works, what `tr`
  does — and picks the natural primitive composition, *including regex
  primitives when that's the natural shape*.
- **Backends only implement primitives.** A backend never re-derives
  "what does `wc -l` mean?" — that would duplicate the decision (and risk
  drift) in every language.

## What this means for "line count"

`wc -l` is a **newline count** (each line is terminated by `\n`), not a
naive `split('\n').length`. Checked against bash:

```
echo -e "line1\nline2\n" | wc -l   # 3
echo -e "line1\nline2"   | wc -l   # 2
```

So `wc -l` is **not** `ArrayLen(Split(t, '\n'))` — that's off by one on the
trailing newline. The natural lowering is a **regex count** primitive:

```
wc -l  →  RegCount(t, /\n/)
```

`RegCount` is a universal primitive every regex-capable language has
(`t.match(/\n/g).length`, `() =~ tr/\n//`, etc.). The core picks it; the
backend just implements `RegCount` once. Nobody re-decides bash's `wc -l`
semantics except the core.

## The universal primitives

A tiny vocabulary, each a one-liner per backend. Includes regex primitives
(the core may choose them when natural):

| Primitive | JS | Perl | Rust | Go |
|-----------|----|------|------|----|
| `StrLen(t)` | `t.length` | `length($t)` | `t.len()` | `len(t)` |
| `Split(t, d)` | `t.split(d)` | `split($d,$t)` | `t.split(d)` | `strings.Split(t,d)` |
| `ArrayLen(a)` | `a.length` | `scalar(@a)` | `a.len()` | `len(a)` |
| `Join(a, d)` | `a.join(d)` | `join($d,@a)` | `a.join(d)` | `strings.Join(a,d)` |
| `ArrayIndex(a, i)` | `a[i]` | `$a[$i]` | `a[i]` | `a[i]` |
| `SubStr(t, o, n)` | `t.substring(o,n)` | `substr($t,$o,$n)` | `&t[o..o+n]` | `t[o:o+n]` |
| `Case(t, upper)` | `t.toUpperCase()` | `uc($t)` | `t.to_uppercase()` | `strings.ToUpper(t)` |
| `Contains(t, p)` | `t.includes(p)` | `index($t,$p)!=-1` | `t.contains(p)` | `strings.Contains(t,p)` |
| `Trim(t)` | `t.trim()` | `s/^\s+|\s+$//g` | `t.trim()` | `strings.TrimSpace(t)` |
| `Repeat(t, n)` | `t.repeat(n)` | `$t x $n` | `t.repeat(n)` | `strings.Repeat(t,n)` |
| `RegCount(t, re)` | `(t.match(re)||[]).length` | `() =~ re` | `re.find_iter(t).count()` | `regexp.FindAllString` |
| `RegReplace(t, re, s, g)` | `t.replace(re,s)` | `s///` | `regex.replace` | `regexp.ReplaceAll` |

~12 primitives, all universally available. **No backend-specific command
logic anywhere.**

## Compositions (the core builds these)

The core lowers each shell command to a tree of primitives:

| Shell | Lowered composition | Why |
|-------|---------------------|-----|
| `wc -l` | `RegCount(t, /\n/)` | newline count, NOT split+len |
| `wc -w` | `ArrayLen(Split(t, /\s+/))` | word count = split+len |
| `wc -c` | `StrLen(t)` | char count = length |
| `cut -d, -f2` | `ArrayIndex(Split(t, ","), 1)` | field = split+index |
| `head -n 5` | `Join(ArraySlice(Split(t,"\n"),0,5), "\n")` | = split+slice+join |
| `basename p` | `ArrayIndex(Split(p, "/"), -1)` | last path segment |
| `dirname p` | `Join(ArraySlice(Split(p,"/"),0,-1), "/")` | all but last segment |
| `${#t}` | `StrLen(t)` | length |
| `tr 'A-Z' 'a-z'` | `Case(t, false)` | case transform |
| `tr 'a' 'b'` | `MapChars(t, a, b)` | char map |
| `sed 's/x/y/'` | `RegReplace(t, x, y)` | regex replace |
| `grep -q P` | `Contains(t, P)` | contains |
| `[[ $x == P* ]]` | `StartsWith(t, P)` | starts-with |
| `echo X \| xargs` | `Trim(t)` | trim |

## The benefit

1. **Backends stay tiny.** Each implements ~12 primitives, one line each.
   It never re-derives "what `wc -l` means" — that lives in the core once.
2. **No per-command drift.** `wc -l` and `wc -w` are different compositions;
   the backend just implements `RegCount` + `Split` + `ArrayLen` once.
3. **Correctness is central.** Each composition (e.g. `wc -l → RegCount`)
   is verified once against bash in the core's unit tests, not re-argued in
   6 backends.
4. **The ESTree path is safe.** A primitive composition lowers to native JS
   (`.split`, `.length`, `.match`) with no `sh2.*` call and no panic, so
   `text_ops` doesn't regress the gate *when the compositions are byte-exact*.

## Where this lives

- The **primitives** are small nodes: StrLen, Split, ArrayLen, Join,
  ArrayIndex, ArraySlice, SubStr, Case, Contains, Trim, Repeat, RegCount,
  RegReplace, MapChars, StartsWith, EndsWith.
- The **compositions** live in `src/transforms/text_ops.rs` — it lowers
  `cut`/`tr`/`sed`/`head`/`tail`/`wc`/`basename`/`dirname` to primitive
  trees. It owns the semantics; backends implement primitives only.

## Guardrail

`text_ops` stays **opt-in** (`DEBASHC_TRANSFORMS=text-ops`) until each
composition is proven byte-exact against bash on the corpus. A wrong
composition (like `wc -l → ArrayLen(Split('\n'))`, off-by-one on trailing
newline) regresses the Estree gate and is a bug in the core, not a backend
problem.

## Backends declare natural operations; the core selects

The core shouldn't force ONE composition on every backend. A backend that
lacks regex (or where regex is costly) should get a **for-loop** lowering for
`wc -l`, not a `RegCount(text, /\n/)`. So:

1. **Each backend declares which primitives it supports** (its capabilities).
   e.g. `{ regex: true|false, arrays: true|false, ... }` or an explicit
   allowlist of the primitive names it renders natively.
2. **The core keeps MULTIPLE candidate lowerings per command**, each using a
   different subset of primitives.
3. **The core selects** the candidate whose primitive requirements the
   target backend supports, preferring the one the backend declares most
   natural (a cost ranking, not a fixed order).

For `wc -l`, the core offers:

| Candidate | Primitives needed | Natural for |
|-----------|-------------------|-------------|
| `RegCount(t, /\n/)` | `RegCount` (regex) | JS, Perl, Rust, Zig |
| `LoopCount(t, '\n')` | `Split`+`ArrayLen`, or a char loop | C, a backend with no regex |
| `ArrayLen(Split(t,'\n')) - …` | `Split`+`ArrayLen`+`EndsWith` | off-by-one handled |

A backend with no regex picks the `LoopCount` (or `Split`+`ArrayLen`)
candidate; a regex-rich backend picks `RegCount`. The CORE picks, informed
by the backend's declared capabilities — no backend re-derives the
semantics, and no backend gets an operation it can't express naturally.

## Capability declaration (shape)

```rust
// Per-backend: which primitive families it renders natively.
struct BackendCapabilities {
    regex: bool,          // can render a RegCount / RegReplace natively
    split: bool,          // can split a string into an array
    char_loop: bool,      // can walk chars and count (no regex needed)
    array_len: bool,      // has an array length primitive
    // ...
}
```

The `text_ops` transform takes the target backend's capabilities and picks
the candidate composition accordingly. `DEBASHC_TRANSFORMS=text-ops` stays
the gate; the per-backend selection happens when the backend requests a
composition.

## Guardrail (extended)

A candidate is only correct when it reproduces bash **byte-exactly for the
given backend**. `wc -l → RegCount` is exact; `wc -l → ArrayLen(Split('\n'))`
is off-by-one. Each candidate is corpus-tested per backend before being
offered. `text_ops` stays opt-in until the candidate table is proven for the
enabled backend.

## Backend node manifests + a core planner (the "solve" idea)

Each backend declares the shIR node types it renders **natively**, as a
plain file — one node type per line. The core reads the manifest and
**plans a reduction** into exactly those node types, recursively.

### Manifest file (one node type per line)

```
# backends/perl/nodes.txt — node types the Perl renderer supports natively
StrLen
Split
ArrayLen
Join
ArrayIndex
Case
Contains
Trim
Repeat
RegCount
RegReplace
SubStr
# ... and a fallback marker
sh2.*           # supports the runtime sh2.* namespace
```

Adding a backend = writing a text file. No code change. Same merge-proof
benefit as the `.node` declarations.

### The core solver

The core keeps a **candidate table**: for each shell command, a priority
list of compositions, each tagged with the node types it requires.

```
wc -l → [
  { nodes: [RegCount],                    priority 0 },
  { nodes: [Split, ArrayLen, EndsWith],   priority 1 },
  { nodes: [Split, ArrayLen],            priority 2 },  # off-by-one — only
                                        # offered to backends that accept it
  { nodes: [sh2.wc],                    priority 9 },
]
```

Given a backend manifest, the solver:

1. **Filters** the candidate list to those whose required nodes ⊆ manifest.
2. **Picks the lowest-priority** candidate among those.
3. **Recurses**: if a candidate's node isn't in the manifest but IS
   composable (e.g. `RegCount` → `Split`+`ArrayLen`), the solver expands it
   into supported leaves.
4. **Falls back** to `sh2.*` (if the manifest lists it) or keeps the original
   command when no candidate is reachable.

### Concrete: `wc -l` across three manifests

| Backend | Manifest has | Solver picks | Renders |
|---------|--------------|--------------|---------|
| JS | `RegCount` | `RegCount(t, /\n/)` | `(t.match(/\n/g)\|\|[]).length` |
| Perl | `RegCount` | `RegCount` | `() = $t =~ tr/\n//` |
| C | no regex | recurses `RegCount → Split+ArrayLen` | a `for`/`memchr` count |

C declares no regex, so the solver **rewrites `RegCount` into a char-loop
count** (or `Split`+`ArrayLen`), never handing C a regex it can't express
naturally. The semantics (newline count) live in the core's catalogue once;
each backend just implements its declared leaves.

### Why this beats one fixed lowering

- **No forced shapes.** A regex-fearing backend gets a loop; a loop-averse
  backend gets `RegCount`. The core decides per manifest.
- **Recursion makes the manifest minimal.** A backend only declares LEAVES
  it renders; the core expands every composite into those leaves. A backend
  that has only `Split`+`ArrayLen` still supports `wc -l`.
- **Correctness stays central.** Each candidate is corpus-proven per node
  requirement; the off-by-one `Split+ArrayLen` candidate is only offered to
  backends whose manifest explicitly accepts it.
- **Fallback is uniform.** `sh2.*` or the original command is the terminal
  node when a backend can't reach the semantics.

## Implementation sketch

```rust
// Per-backend, loaded from backends/<lang>/nodes.txt.
struct Capabilities { nodes: HashSet<String>, has_sh2: bool }

// The catalogue: command → ordered candidates.
fn candidates(cmd: &str) -> Vec<Candidate>;
// Candidate = { nodes: Vec<NodeName>, build: fn(&Ctx) -> IrExpr }

// The solver.
fn plan(cmd: &str, cap: &Capabilities) -> Option<IrExpr> {
    for cand in candidates(cmd) {
        let plan = solve(cand, cap)?;       // recurse into composite nodes
        if plan.renders_within(cap) { return Some(plan); }
    }
    None // fall back to sh2.* / original
}
```

This is the shape `text_ops` grows into: a backend-manifest-driven planner
instead of a single hard-coded lowering.
