# UU-FFI — In-Process uutils Coreutils for Backend Runtime Support

Status: **IMPLEMENTING** (2026-09-06). The foundation is landed as code:
`runtime/build_uu_ffi.sh`, the `runtime/uu_run.{h,c}` wrapper, and two
green gates (`tests/parity_vs_bash.sh`, `tests/c_backend_uu_ffi.sh`, §9
executed below). The C backend wiring is env-opt-in (`SH2_UU_FFI=1`),
byte-identical by default. Still open: Python/Go/Perl adoption, the
per-backend corpus-gate sign-off, and the toolchain/`.so`-deployment
decisions (§7/§8). The plan (PLAN.md) remains the authority.

This doc captures the design; §10 records what is implemented and its
green status.

## 0. tl;dr

Backends today lower *external* commands (`sort`, `sed`, `grep`, `wc`,
`ls`, `cat`) by fork/exec — C calls `system()`, Python calls
`subprocess`, Perl uses `qx{}`, Rust shells out. The `builtin` op has a
native arm in each renderer, but the genuinely-external commands still
re-route to a subprocess, re-deciding semantics per backend, with
parity-drift risk (AGENTS.md rendering-parity policy).

`uu-ffi` exposes a **C ABI over the uutils coreutils** so those external
commands can be called **in-process** as a shared implementation from any
language that can bind C. The prototype proves capture (fd 0-2 redirect)
works from C and Python, and surfaces the one real complication: large
output deadlocks the naive run-then-read pattern and needs a concurrent
draining reader.

**Recommendation:** adopt uu-ffi as a *long-term, per-backend native arm*
for a **curated list of genuinely-external, side-effect-free commands**,
not as the backend wholesale. Shell builtins (`cd`, `export`, `read`,
`local`, `declare`, `shift`, …) **stay native** — uu-ffi cannot expose
them (no such coreutils utility exists) and their in-process effect is
exactly what the target's native `chdir()`/variable-assignment already
does for free. The **estree/JS backend opts out entirely** (browser, no C
FFI; it already passes the corpus). A parallel `feat_wasm` path exists in
the uutils monorepo for the browser, out of scope here.

---

## 1. Context: how backends handle external commands today

The shIR `builtin` op (`transforms/builtin.rs`, the shared
`harness/builtins.json` 69-entry namespace, `builtin-lift`
`exec → builtin`) is the single contract. Each renderer decides how to
render `builtin("cmd", args)`:

| Backend | External-command rendering today | `builtin` native arm |
|---|---|---|
| C | `system(_sh_wrap)` (fork/exec, `_GNU_SOURCE` shell string) | a few (`echo`, `printf`, `true/false`, `read`) |
| Python | `subprocess` / `__sh_exec([...])` / `subprocess.check_output` | `echo`→`print`, `printf`→`sys.stdout.write`, `true/false` |
| Perl | `qx{}` | native subs (partial) |
| Rust | `bash -c` via `__sh_run` | — (reference) |
| **estree/JS** | **consumer implements `sh2.exec`**; browser, no spawn | sync table + native twins |

The M8/`builtin-lift` ladder already pushes as many commands as possible
down to native/`builtin` arms. The residue — commands the renderer can't
express natively in the target language's idioms — falls back to
fork/exec with a shell string. That is exactly the case AGENTS.md calls
"LAST-RESORT" and catalogues per backend in `docs/backend-<lang>-limitations.md`.

**uu-ffi's opportunity:** when the fallback WOULD be a fork/exec of a
genuine external utility, replace the subprocess with an **in-process
call to the shared uutils implementation**, eliminating the shell string,
the quoting/injection surface, and the per-backend semantic re-decision.

## 2. What uu-ffi actually is

Standalone crate (`/root/src/coreutils/uu-ffi`), a *reimplementation* of
the `uu_ffi` idea that depends only on **published crates.io utility
crates** (`uu_cat`, `uu_echo`, `uu_wc`, `uu_ls`, `uu_sort`, `uucore`,
`sed`, `diffutils`, `awk-rs`, `grep` engine crates), plus a vendored,
embeddable `diffutils` fork (in-process `diff`/`cmp`, no `process::exit`).

Three crates in one workspace:

| Crate | Role |
|---|---|
| `coreutils-ffi` | `cdylib`+`staticlib`, `extern "C"` surface: `uu_run(util, argc, argv)→int`, `uu_multicall(argc, argv)→int` (+ `_w` wide variants on Windows) with `uu_ffi.h`. |
| `coreutils-ffi-api` | Safe Rust wrapper over the C surface. |
| `coreutils-ffi-multicall` | Multicall binary; hard/soft-link as `wc`, `cat`, … to run in-process. |

**Utility coverage is much larger than the README suggests.** Default
features: `cat echo wc ls sort sed`. Optional: `diff cmp` (diffutils),
`awk` (awk-rs), `grep/egrep/fgrep/rg` (ripgrep engine). The C ABI is
identical regardless of which are compiled in.

The `Handler` trait collapses every util's `uumain` (which returns a
`Result`, never calls `process::exit`) to a single `fn(&[OsString])→i32`,
so exit codes come back as a value usable in `&&`/`||` conditions.

## 3. Prototype result (decisive)

Built `libcoreutils_ffi.so` with `cat echo wc ls sort sed awk grep`
(1.96.1 toolchain; the vendored `diffutils` fork currently does **not
compile** against current crates — see §8 blocker). Tested fd 0-2
redirection around `uu_run` to capture stdout/stderr + exit code.

| Test | Result |
|---|---|
| `echo` / `wc -l` / `sort` / `cat` capture | ✅ full output + rc, C and Python |
| `grep` match→0, no-match→1 | ✅ usable as boolean (rc value, not `process::exit`) |
| `sed s///` stream edit, `awk '{print $1}'` | ✅ full 2.9 kB capture |
| `ls /` (28 kB) | ✅ |
| `cat` 5.2 MB through a redirected pipe, naive run-then-read | ❌ **deadlock** |
| same, with a **concurrent draining reader thread** | ✅ 5.2 MB, rc=0 |

**The finding that decides the design:** in-process capture via fd
redirect works, but **output larger than the pipe buffer deadlocks a
naive run-then-read** because `uu_run` blocks writing into a full pipe
while nobody reads. The fix — a reader thread draining fd during the call
— works and is a standard backend idiom. Consequently, any generated code
that captures large output must emit **concurrent drain**, not a bare
`dup2`/`run`/`read` sequence.

## 4. The builtin rule (corrected)

The naive rule is "shell builtins stay native because they mutate
process state." That is **not the reason**. An in-process library call
mutates the *same* process — that is exactly why it works. The real
reasons builtins are NOT uu-ffi targets:

1. **uu-ffi has no such utility to expose.** `cd`, `export`, `read`,
   `local`, `declare`, `mapfile`, `shift`, `readarray`, `set`, `unset`,
   `return`, `trap`, `eval`, `source`, `let`, `break`, `continue`,
   `readonly`, `typeset`, `command`, `.`, `:` are **shell-language
   constructs, not utilities.** GNU coreutils (mirrored by uutils) ships
   none of them as a binary; there is literally nothing to bind.
   (`cd` *cannot* be an external binary — cwd isn't inherited by callers —
   which is why the shell keeps it as a builtin.)
2. **`uu_run`'s return channel can't carry their result.** `read`,
   `mapfile`, `readarray` write one-or-more named values back into shell
   variables; `uu_run` returns a single `i32` exit code. Structurally
   incompatible independent of the mutation question. `export`/`declare`
   mutate the caller's env/var table via the process env — a second
   source of truth the transpiled program must keep coherent.
3. **Native is the same thing for free.** For `cd`, the "in-process
   uu-ffi call" is byte-for-byte what the target's native `chdir()` /
   `os.chdir()` already does, minus the FFI hop and the `.so` dependency.

**The rule that survives scrutiny:**

> *If it's a real external utility (`sort`, `sed`, `grep`, `awk`, `wc`,
> `ls`, `cat`, `diff`, `cmp`, `seq`, `head`, `tail`, …), uu-ffi may be a
> shared in-process implementation. If it's a shell builtin (`cd`,
> `read`, `local`, `declare`, `shift`, …), the native call already IS the
> in-process call — emitting it natively is strictly simpler.*

This is the same shape as the M8 ladder: prefer native idiom, then a
shared in-process utility call, fork/exec as last resort.

## 5. Backend-by-backend integration plan (all behind the `builtin` op)

The shIR contract stays `builtin("cmd", args)`; uu-ffi is a **renderer
choice**, never a transform change. No shIR/transform modification.

| Backend | Does uu-ffi make sense? | How |
|---|---|---|
| **Python** | ✅ first consumer | `ctypes`/`cffi` `CDLL`, load `.so`, call `uu_run`; emit fd-redirect + a draining reader thread around it. Cheapest deployment (a `.so` alongside the script). |
| **Go** | ✅ | `cgo` binds the staticlib; emit a goroutine draining the output buffer. cgo has compile-time friction (note). |
| **C** | ✅ | strongest fit — a native call, no FFI hop, can use `pipe`+`dup2`+a thread or a temporary-file fallback natively. Replaces `system(_sh_wrap)` for external commands. |
| **Perl** | ⚠️ | feasible via FFI/DynaLoader, but today Perl is **zero external deps** (pure `qx{}`+subs). Adding a `.so` is a real deployment downgrade; gate this one on demonstrated corpus value. |
| **Rust** | ❌ skip | shares the uutils crates directly (its `Cargo.toml` → `uu_*`); the C ABI layer buys nothing. |
| **estree/JS** | ❌ skip | browser; no C FFI; already passes corpus; consumer owns `sh2.exec`. Use the uutils monorepo `feat_wasm` path (separate doc) — or leave alone. |

**Which commands route through uu-ffi:** only the "genuinely external,
side-effect-free" residue of the M8/ladder that a given backend cannot
express natively — e.g. `sort`, `sed`, `grep`/`egrep`/`fgrep`, `awk`,
`wc`, `ls`, `cat`, `diff`, `cmp`. Commands a backend already lowers to a
native arm (`echo`→`print`, `printf`→`sys.stdout.write`, `sleep`→timer,
`local`→`let`) must stay native — uu-ffi only *replaces a fork/exec*, not
an existing native arm.

## 6. Capture semantics for generated code (the deadlock lesson)

Any backend that emits an in-process `uu_run` for a command whose stdout
is captured, piped, or substituted must emit, alongside the call:

- fd 1/2 redirected to a pipe (and restored);
- a **concurrent reader** that drains the pipe while the command runs
  (a thread in C/Go/Python; a temp-file sink is an acceptable
  no-thread alternative — see §7);
- the exit code preserved as a value, and made available for
  `&&`/`||`/`$?` semantics.

Reference template emitted (Python, abbreviated):

```python
out_r, out_w = os.pipe(); so = os.dup(1)
os.dup2(out_w, 1); os.close(out_w)
# concurrent drain (thread or async) — NOT run-then-read
# rc = lib.uu_run(b"sort", argc, argv)
os.dup2(so, 1); os.close(so)
```

Backends that already have a capture path (Python `subprocess` capture,
estree capture) should reuse its plumbing rather than add a parallel one.

## 6.1 The priority caveat: uu-ffi is better than fallback, not better than native

An in-process library call is indeed far cheaper than a fork/exec — no
`process`/`_sh_wrap` shell-string construction, no quoting/injection
surface, no process-spawn overhead — and it shares one implementation
across backends. **That is the argument for uu-ffi.** But it is a
*fallback upgrade*, not a destination.

The primary goal of every backend remains to lower shell **into the
idiomatic code of the target language** — native `print()` / `Math.*` /
string-array ops / `chdir()` — **free of any source-language-specific
runtime** (no bash interpreter, and ideally no `sh2.*` support library) so
the emitted program reads and behaves like hand-written target code.

The M8 / `builtin-lift` ladder expresses this priority exactly:

```
1  native target idiom           (echo → print, sleep → timer, local → let)
2  shared in-process coreutils    (uu-ffi: a library CALL, not a subprocess)
3  fork/exec with a shell string  (LAST-RESORT)
```

uu-ffi occupies level 2: strictly preferred to level 3 (cheaper, shared,
safer), but strictly subordinate to level 1. **A backend must not stop
lowering a command because uu-ffi makes it cheap** — the ladder's
reduction effort continues; the uu-ffi seam is what remains after native
lowering has done its work. The corpus cell, not the presence of a cheap
utility call, is the success signal.

This is the same caveat `CROSS_BACKEND_RUNTIME.md` applies to its own
pure-CPU polyfill seam: "Native idioms stay preferred — the M8 lowering
ladder is unchanged. The polyfills are the fallback seam." uu-ffi is the
*process-bound* cousin of that seam (it cannot be authored in bash and
re-emitted — it is a precompiled foreign library), and it carries one
cost the polyfill seam does not: a source-language-specific **`.so`
runtime** that the generated program now depends on. That dependency is
precisely the "source-language-specific runtime" an idiomatic
translation wants to avoid, so it further argues for keeping the uu-ffi
arm narrow and reserving it for the genuinely-external residue — not the
whole builtin set.

## 7. Design questions to resolve before implementation

1. **Stream capture vs buffer argument.** A C library call must capture
   through fds/threads (the C ABI only passes `argv`). A wasm build of
   the *same* uutils could instead take an **in-memory buffer** for
   output (no pipe, no deadlock). For the native backends this is fine;
   for any future browser/wasm consumer, prefer the buffer shape. Note
   this as the primary difference between "UU-FFI (native C ABI)" and
   "uutils as a wasm library" — the latter needs no fd capture.
2. **Concurrent-drain dependency.** Requiring a threading primitive in
   generated code is a new constraint (some targets are single-threaded
   WASI/browser contexts). **Temp-file sink** is the thread-free
   alternative: point fd 1 at a temp file, run, read the file back. Trade
   disk I/O for no threads — acceptable for non-interactive scripts; the
   M8 ladder's `Spawn` purity tag can guide when a temp-file sink is safe
   (no interactive consumer, no unbounded latency requirement).
3. **`.so` deployment.** Each consuming backend ships a shared object.
   Decide: a single universal `libcoreutils_ffi` binary artifact released
   alongside the transpiled program, vs linking the staticlib into the
   binary (C/Go) vs requiring the user to install it (Python/Perl). The
   "emit self-contained source" ideal (§4 CROSS_BACKEND_RUNTIME) conflicts
   with this; this is the main adoption cost.
4. **Version/ABI pinning.** uu-ffi is 0.1.0, pins `uucore 0.10.0`,
   needs the `[patch]` vendored diffutils, and the README itself notes
   `uumain` ABI churn. **All consuming backends break together** on an
   upstream bump. Fix the exact dependency set + toolchain in the
   consuming repos' CI, and treat the C ABI as a thin, stable seam.
5. **Correctness gate.** Before any backend adopts it, prove parity:
   the SAME (input, argv) must produce byte-identical stdout and the same
   exit code under "uutils in-process" vs the current fork/exec baseline,
   across a corpus cell's worth of invocations. Add this as a
   workspace-side gate (a `fail`-style cell comparing the two).

## 8. Blockers (as of 2026-09-06)

- **Vendored `diffutils` fork does not compile** against current
  crates.io (`unresolved import diffutilslib::diff`, `Mismatched types` —
  API drift in the fork). `diff`/`cmp` are not available until fixed.
- **Toolchain**: uu-ffi needs rustc ≥ 1.88 (uses edition 2024); the
  workspace default is 1.85. Builds require an explicit
  `rustup run 1.96.1 cargo build` (used for the prototype).
- **Deployment**: no release pipeline exists for the `.so` — prototype
  links `target/release/libcoreutils_ffi.so` directly.
- **JS/browser**: uu-ffi has no wasm target; uutils monorepo `feat_wasm`
  is the browser path (separate consideration, out of scope here).

## 9. Recommendation & sequencing

Long-term goal; proven viable; not a drop-in now. Suggested order:

1. **First consumer: C backend.** Replaces `system(_sh_wrap)` for
   external commands with a native in-process call + concurrent drain.
   C has the strongest fit and the clearest parity story.
2. Then **Python** (ctypes; already prototyped). Then **Go** (cgo).
3. **Perl last** (deployment-cost gate). **Rust and estree/JS opt out.**
4. For every backend: fix diffutils, pin versions/toolchain, add the
   correctness-vs-fork parity gate (§7.5), and keep the `builtin` op as
   the only contract.

---

## Appendix: the two "is there a difference" answers (context for future readers)

- **Does uu-ffi help the JS backend?** No. uu-ffi is a *native C ABI*
  crate; a browser has no C FFI. The estree backend already passes the
  corpus and its `sh2.exec` is a contract the consumer implements. If JS
  ever wants in-process coreutils, it is the uutils monorepo's `feat_wasm`
  (a `.wasm` module), delivered as a library with in-memory buffers — a
  different path than uu-ffi.
- **Execing a wasm binary vs calling a wasm library?** At the runtime
  level there is little difference (wasm has no process; both are an
  instantiated-module call). The *real* differences are the isolation
  model (a wasm *binary* runs as its own sandboxed wasi instance with its
  own preopens/fds — safer for the state-mutating builtins; a wasm
  *library* shares the host instance — state-sharing is a hazard) and the
  I/O model (binary → stream/fd capture, the deadlock concern returns;
  library → in-memory buffer, no capture needed). This is why the native
  backends use the C-ABI library shape (buffer/thread capture) and any
  browser consumer would use the wasm-library buffer shape.

---

## 10. Implemented status (2026-09-06)

The foundation is landed and gate-green. A reader reproducing this needs
the uu-ffi source (default `/root/src/coreutils/uu-ffi`) and a rustc that
builds it (edition 2024, `>= 1.88`; the workspace default 1.85 is not
enough — the build script uses the toolchain from `UU_FFI_TOOLCHAIN`,
default `1.96.1`).

### 10.1 Artifacts in `runtime/`

| Path | What |
|---|---|
| `runtime/build_uu_ffi.sh` | Builds `libcoreutils_ffi.so` + `uu_ffi.h` into `runtime/lib/`. Features: `cat echo wc ls sort sed awk grep`. `diff`/`cmp` still excluded (§8 blocker). |
| `runtime/uu_run.h` | The seam: `sh2_uu_run(argc, argv)`, `sh2_uu_capture(argc, argv, buf, cap, &len)`. |
| `runtime/uu_run.c` | Implementation: fd-1 redirect to a pipe, drained on a POSIX thread **concurrently with** `uu_run` — the prototype's deadlock fix (§3/§6). |
| `runtime/tests/parity_vs_bash.sh` | §7.5 gate: same argv via uu-ffi vs real coreutils; asserts byte-identical stdout + exit code. 8 cases green, incl. a loud-refusal check. |
| `runtime/tests/c_backend_uu_ffi.sh` | End-to-end C-backend gate: default render stays shell-out (no uu_run) **and** matches bash; `SH2_UU_FFI=1` render uses `sh2_uu_run` **and** matches bash. |

### 10.2 C backend (first consumer, env-opt-in)

`src/c_backend.rs` adds a `need_uu` flag + `uu_run_opt()`:

- **Only `SH2_UU_FFI=1` at render time changes output.** Default
  (env-unset) render is byte-identical to before — `uu_run_opt` returns
  `None` immediately, the existing `shell_exec` path is untouched. The
  no-regression gate holds.
- When set, a **statically-known** genuinely-external command
  (`cat wc ls sort sed awk`), with **all-static string words and no
  env/Object prefix**, renders as an inline `char *_sh_uu_avN[]` + a
  `sh2_uu_run(...)` statement instead of `_sh_site_N`/`bash -c`. Any
  dynamic word, array, or env-prefix keeps the shell site.
- The generated C `#include <uu_run.h>`; it must link the runtime
  (§7.3 deployment): `gcc -I runtime -I runtime/lib out.c runtime/uu_run.c
  runtime/lib/libcoreutils_ffi.so -lpthread -Wl,-rpath,runtime/lib`.

Both gates currently pass:
- `runtime/tests/parity_vs_bash.sh` → 8 cases, 0 divergences.
- `runtime/tests/c_backend_uu_ffi.sh` → 5 checks, 0 failures (default
  and uu-ffi modes each match bash).

### 10.3 One real dependency bug fixed

The parity gate caught that uu-ffi's `grep` frontend defaulted
`line_numbers = true`, diverging from GNU grep (which only numbers with
`-n`/`-H`). Fixed in the uu-ffi crate (`coreutils-ffi/src/lib.rs`, default
→ `false`); the rebuilt `.so` reflects it. This is the gate doing its job
— a divergence is a bug, not something to bless.

### 10.4 Not yet done (deliberately)

- Python / Go / Perl adoption (the `.so` deployment cost must be
  accepted per backend; Perl is gated on corpus value).
- estree/JS: out of scope (browser, no C FFI, uu-ffi has no wasm target;
  the uutils monorepo `feat_wasm` is a separate path).
- `diff`/`cmp` (vendored diffutils fork does not compile against current
  crates).
- A release pipeline for the `.so` and a per-backend corpus-gate
  sign-off (the "cell green under SH2_UU_FFI" proof) before the C-arm is
  enabled outside the env flag.
