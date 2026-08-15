# Embed Contract Norms (purify — native replacement of system-like constructs)

The embed profile renders a shell **snippet** as a host-language **fragment**
inside a surrounding program — the purify design (PLAN.md §10). The snippet
is a closed mini-program: it gets the full shIR treatment (parse → A1 →
analysis → render), and the fragment is spliced back into the host file at
the construct's byte span. **The host text outside the span is never parsed,
never re-rendered, never understood by the core** — that is the preservation
contract, mechanically gated (`purified output minus replaced spans == input
minus marked spans`).

Status: DRAFT — Stage 1 implements the Perl `Backtick` profile
(`shir_to_perl_embed`, `src/ir.rs`). `System`/`Popen` and the other host
languages are reserved.

## 1. The record: A1 annotations + one `embed` block

Per construct site, the splice engine hands the core:

```jsonc
{
  "a1": {                       // the snippet's full A1 — the existing contract
    "stmts": [ /*…*/ ],
    "var_types": [ /*…*/ ],     // A2 Int/Str
    "var_const": [ /*…*/ ],     // Const/Var
    "var_lifetimes": [ /*…*/ ], // {first, last, escapes}
    "var_lengths": [ /*…*/ ],
    "var_bash_env": [ /*…*/ ],
    "stmt_lines": [ /*…*/ ]
  },
  "embed": {                    // schema: shir-contract/schema.json "embed_block"
    "span": { "start": 1042, "end": 1057 },
    "construct": "backtick",    // backtick | system | popen
    "semantics": { "status_used": true, "context": "scalar" },
    "host_scope": ["x", "lines"],
    "host_imports": { "english": false, "open3": true },
    "mode": { "backtick_newlines": true, "english_names": false }
  }
}
```

The `embed` block is the ONLY host-derived input. Everything in `a1` is
computed by the core's existing pipeline on the snippet.

## 2. Var visibility (the construct rules)

Bash's three construct kinds differ in what the snippet can see and what
leaks back. The fragment must reproduce exactly that:

| construct | reads | writes | fragment rule |
|---|---|---|---|
| backtick / `$()` | subshell inherits the parent's vars | discarded (subshell) | host vars visible to reads; writes must NOT escape |
| `system("cmd")` | child sees only env + exports | discarded | reads via `%ENV`-semantics; nothing escapes (reserved) |
| `popen` / `open("\|cmd")` | stream + env | n/a | handle semantics (reserved) |

### Stage-1 (backtick) declaration rules — `src/ir.rs` `shir_to_perl_embed`

Per snippet variable, from the A1 read/write sets (first-seen order,
Vec-based — deterministic; the legacy `Generator`'s HashSet order was
30/30 flaky across processes):

| snippet usage | name ∈ `host_scope` | name ∉ `host_scope` |
|---|---|---|
| read-only | **reuse** — bare `$x` reads resolve to the enclosing lexical (`required_host_bindings += x`) | `my $x = '';` (bash unset = empty) |
| written | `my $x = $x;` **copy-in** — reads see the host value, writes stay fragment-local (`required_host_bindings += x`) | `my $x;` |
| written, subshell-restored | same as written (the `Subshell` emit's save/restore composes with the copy-in) | same |

**The whole fragment is wrapped in a `do { … }` block.** This is
load-bearing, not cosmetic: in the host's own scope a second `my $x` after
the host's `my $x` would mask-**reuse** the same pad slot — the snippet's
writes would leak into the host lexical (verified: `my $x = "5"; my $x =
$x; $x = 6` leaves the host `$x` at 6 and warns "masks earlier declaration").
Inside `do { … }` the copy-in is a fresh lexical: writes die with the block,
exactly bash's subshell semantics. purify.pl's `__bt(do { … })` wrapper is
the same shape.

### The bindings gate

`required_host_bindings` = the names the fragment actually reads from the
host scope (reuse + copy-in). **Gate: `required_host_bindings ⊆
host_scope`.** A fragment that emits a bare `$x` for a name the caller did
not list is a renderer bug and a hard failure. The v2 upgrade: derive the
share-set from the shIR analyses (`var_lifetimes[].escapes`, the
lifted_numeric/lifted_string sets) instead of the read/write sets — the
machinery is the same, the verdicts get sharper (PLAN §10).

## 3. Post-render rewrites (renderer-owned, mirroring purify heuristics)

Each rewrite below is a purify.pl regex patch that the renderer now owns —
deterministic, and the refusal scan replaces purify's rejections:

| rewrite | condition | why |
|---|---|---|
| `$main_exit_code = $CHILD_ERROR = X;` → `$CHILD_ERROR = X;` | always | the standalone exit tracker is dead in an embed; the `$?` mirror stays |
| drop `chomp $_r;` in command substitution | `mode.backtick_newlines` | Perl `qx` does NOT strip trailing newlines; bash `$()` does — the enclosing Perl backtick's semantics win |
| `$INPUT_RECORD_SEPARATOR → $/`, `$OS_ERROR/$ERRNO → $!`, `$EVAL_ERROR → $@` | `!mode.english_names` | host file may not `use English` |
| prepend `our $CHILD_ERROR = 0;` | fragment references `$CHILD_ERROR` | `our` is package-wide; redeclaration is harmless |
| prepend `use Carp;` | fragment references `carp`/`croak`/`cluck`/`confess` (command emulations do, on error paths) | the standalone preamble imports Carp; `use` is compile-time and package-wide, and a duplicate in a host that already imports it is a silent no-op (purify.pl's import-injection, minus the regex) |

### Refusals (the analysis-driven rejection class)

The renderer REFUSES a site (the caller falls back, e.g. to
`exec('sh','-c',…)`) when the snippet needs something the embed can't
provide:

- `IrStmt::Exit` / a bare `exit` in the fragment — would terminate the HOST.
- function definitions — need a host-scope binding (collision).
- background jobs — fork + `exit $main_exit_code`.
- residual `$main_exit_code` / `$__argc` / `$__nocasematch` / `$ls_success`
  / `$DATE_SNAPSHOT` references — standalone-only preamble vars.
- `say` usage — the host may lack `use feature 'say'`.

These replace purify's `$DATE_SNAPSHOT`/`system()` regex rejections —
same fallback, but deterministic and enumerable.

## 4. Determinism

`shir_to_perl_embed` output is byte-identical run-to-run (verified 30/30
processes): declarations follow the Vec-based first-seen order, and the
rewrites are fixed-string replacements. Gate: `embed_fragment_is_deterministic`.

## 5. Gates (workspace)

1. `cargo test --lib` — `embed_*` unit tests (determinism, bindings gate,
   copy-in, no-preamble, main_exit collapse, English normalization,
   refusals).
2. purify-twice byte-identity (proposed; red on the legacy `--inline` path
   today, green when purify.pl's backtick path moves to the embed profile).
3. Preservation invariant: `output minus replaced spans == input minus
   marked spans` (proposed).

## 6. Stage roadmap

1. ✅ Stage 1: `shir_to_perl_embed` (Perl backtick profile) + unit tests +
   `parse --perl-embed` CLI hook (`PURIFY_SCOPE` env for manual testing).
2. ✅ Stage 2: **otranspilerl `--embed-perl`** — `render_embed(a1, EmbedOpts)`
   + CLI flags `--embed-perl` / `--scope-vars a,b,c` / `--backtick` /
   `--english` (fragment on stdout, `REQUIRED`/`REFUSE` on stderr so stdout
   stays splice-clean); 5 CLI tests (`embed_*` in `otranspilerl/src/lib.rs`),
   `EmbedConstruct::System`/`Popen` still reserved.
3. ✅ Stage 3: **purify.pl backtick swap (opt-in, `PURIFY_EMBED=1`)** —
   `convert_shell_to_perl_embed` calls `otranspilerl-cli --embed-perl
   --backtick --scope-vars <file-wide my/our harvest>`; on REFUSE it falls
   back to the legacy path (graceful degradation). The fragment is wrapped
   in `__bt(do { … })` and RUN IN A FORKED CHILD with stdout on a pipe
   (`open '-|'`): external commands fork grandchildren that inherit the
   pipe's fd 1 (a `local *STDOUT` scalar/file capture does NOT rebind fd 1
   — verified `wc -l` leaked to real stdout), and the fork gives true
   bash-subshell semantics for free. Fragment preamble (`our $CHILD_ERROR
   = 0;` / `use …;`) is extracted and injected at FILE level (a `use`
   inside the `__bt(do{…})` expression is a syntax error).
   **Corpus A/B (examples.impurl, 33 purify-relevant files): legacy 8/33,
   embed 22/33** — the remaining 11 are inherited shIR renderer emulation
   gaps that reproduce via standalone `file --perl` (printf `\n` escapes,
   `mkdir -m`, env-assign echo, …), NOT embed-profile bugs. Purified
   output is byte-deterministic across runs (3/3 verified).
4. ⏳ shIR verdict upgrade: `required_host_bindings` from
   `var_lifetimes[].escapes` + lift sets (PassContext) instead of the
   read/write sets.
5. ⏳ Generic profile: per-host-language construct finders + the
   construct-shaped fragment API (`--embed=<lang> --construct=…`) + the
   preservation gate for every host language.

## 7. Known pre-existing limitation (not embed-specific)

A command substitution whose INNER command the renderer falls back to the
`bash -c` capture path can emit the emulated Perl body as the bash command
(`open(my $__fh, '-|', 'bash', '-c', q(sub { … }))`) — bash reports
`sub: command not found`. This reproduces byte-for-byte through the
standalone `debashc file --perl` (it is a `shir_to_perl` capture-path bug,
not the embed profile). The corpus does not currently exercise this shape
(`$(…)` in the middle of a double-quoted string with an emulable inner
command). Fixes live in the shared renderer; the embed smoke matrix marks
it as an inherited limitation.
