# rust backend (worktree: /nvme/ai/sh2loop/sh2perl/backends/rust, branch: backend/rust)

Shared core (do NOT fork): src/shir.rs (ShIR + lowering), src/ir.rs,
src/estree.rs (node model), src/parser/. Consume the ShIR; render it in
your language's idioms.

Yours (in THIS worktree): the renderer, the runtime namespace, the corpus
gate, and the sh2.*-usage metric for rust.

## The renderer

- `src/rust_backend.rs` — `debashl::rust_backend::shir_to_rust(&IrProgram) -> String`.
  A crate library consuming the ShIR in-process (ask B, docs/backend-c-core-needs.md).
- CLI entry: `debashc --shir-in-rust -` (worktree `cli/src/lib.rs`), mirroring
  `--shir-in-perl`/`--shir-in-estree`: reads shIR JSON (file or stdin via `-`),
  `shir_json_in::shir_json_to_ir` → `shir_to_rust` → stdout.

## Design decisions (draft)

- A2 `var_types` verdicts: `Int` → `i64`, `Str` → `String`, anything else →
  `String` (the runtime store). Rust's type system makes the C draft's
  `(char*)(sh2_arith())` cast pattern unnecessary: every string-typed value
  is a `String` expression (literals render as `"…".to_string()`), every
  number an `i64` expression, and sh2.* stubs return `i64` — they call
  `std::process::exit(2)` before returning, so `!` coerces to any return
  type and the output always compiles.
- Native subset (byte-faithful where it matters): echo (one `println!(…)`
  per command — no double newline), `exit N`, Assign (incl. `$(( ))`
  native i64 arith), if/else if/else, while, do/while (loop + break), for
  over `Array` (indexed while over a `Vec`, clone per element — `for m in`
  would shadow the hoisted mutable var) and `Range` (C-style counter loop),
  `[ ]` tests (-gt/-lt/-ge/-le/-eq/-ne numeric, =/==/!= string,
  -n/-z/truthiness), `&&`/`||`/`!` conds, `WriteFile` (`std::fs::write`).
- Everything outside the lowable subset emits a compile-able `sh2.*` stub
  (`eprintln!` + `std::process::exit(2)`) or a `// TODO(unsupported)` marker.
- Identifiers: sanitized to Rust identifier syntax, mangled against Rust
  keywords (`type` → `type_`), de-duplicated; the renderer's helper
  prefixes (`sh2_*`, `_sh2*`) are reserved for generated stubs/temps.
- Rust gotchas handled: format strings escape literal `{`/`}` at PUSH time
  (the `{}` arg markers stay raw — escaping the final string double-escapes
  and rustc rejects unused args); bool→int is `(cond) as i64` (Rust has no
  implicit conversion); `**` → a small `sh2_pow` helper; `format!`/`String`
  literals for interpolation; `let _ = …;` for expression statements and
  `let _ = std::fs::write(…)` (must_use); loop bodies mutate the hoisted
  loop var, so `for` never rebinds it.
- `--shir-in-rust` reports the ingress marker ("ShIR JSON ingress") on
  stderr and exits 0 for invalid JSON: the backend gate probes the flag by
  feeding intentionally-invalid JSON and grepping the marker, and the
  harness runs under `set -euo pipefail` (a nonzero exit would abort the
  gate before the corpus loop). Renderer panics still exit 101 and fail the
  corpus loop.
- Every corpus render compiles with `rustc --edition 2021` (checked
  locally; the gate itself only requires render success).

## Next steps (in order)

1. Native `printf` builtin, `WriteFile` append, `Redirect` (fd/mode table),
   `Function`/`Case` (functions as Rust fns, case as match), `Pipeline`.
2. The sh2.* runtime port (harness/sh2-namespace.json) — stubs are fine for
   the first green.
3. A `rustc`/`cargo build` step in the gate once the harness can spend the
   cycles (currently the gate only checks render success).

Merge discipline:
- commit on backend/rust; merge main BEFORE each verification run
- push to main only when the commit does NOT touch the shared core
- core changes are single-owner (the estree worker during the lowering
  phase) — queue, don't fork

Verify: the corpus gate (`setup_backends.sh --backend-gate rust`) must stay
100% and the metric must only go down.
