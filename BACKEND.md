# c backend (worktree: /home/llm/sh2loop/sh2perl/backends/c, branch: backend/c)

Shared core (do NOT fork): src/shir.rs (ShIR + lowering), src/ir.rs,
src/estree.rs (node model), src/parser/. Consume the ShIR; render it in
your language's idioms.

Yours (in THIS worktree): the renderer, the runtime namespace, the corpus
gate, and the sh2.*-usage metric for zig.

## The renderer

- `src/zig_backend.rs` — `sh2perl core::zig_backend::shir_to_zig(&IrProgram) -> String`.
  A crate library consuming the ShIR in-process (ask B, docs/backend-c-core-needs.md).
- CLI entry: `shir_render --target zig -` (worktree `cli/src/lib.rs`), mirroring
  `--shir-in-perl`/`--shir-in-estree`: reads shIR JSON (file or stdin via `-`),
  `shir_json_in::shir_json_to_ir` → `shir_to_zig` → stdout.

## Design decisions (draft)

- A2 `var_types` verdicts: `Int` → Zig `i64`, `Str` → `[]const u8`, anything
  else → `[]const u8` (the runtime store — shell vars are strings; Zig has
  no `any`, so the store is the string map). `sh2ToInt`/`sh2IntStr`/
  `sh2Truthy` convert at the type boundaries and the output always compiles.
- Native subset (byte-faithful where it matters): echo (one `stdout.print`
  per command — no double newline), `exit N`, Assign (incl. `$(( ))` native
  i64 arith), if/else if/else, while, do/while, for-in over `Array`/`Range`
  iterators, `[ ]` tests (-gt/-lt/-ge/-le/-eq/-ne numeric, =/==/!= string —
  incl. the parser's space-stripped `"$a"="b"` form, -n/-z/truthiness),
  `&&`/`||`/`!` conds, `WriteFile` (non-append).
- Everything outside the lowable subset emits a compile-able `sh2.*` stub
  (`sh2TODO` → stderr + `std.process.exit(2)`) or a `// TODO(unsupported)`
  marker.
- Identifiers: sanitized to Zig identifier syntax, mangled against Zig
  keywords/builtins (`fn` → `fn_`, `a-b` → `a_b`), de-duplicated.
- Zig gotchas handled: no implicit conversions (`@intFromBool` for
  bool→int, `sh2IntStr` for int→string, `sh2B2S` for bool→string);
  string equality/ordering via `std.mem.eql`/`std.mem.order`; runtime
  slice concat via `sh2StrCat` (allocPrint); `**` via `std.math.pow`;
  declared-but-never-read vars get a `_ = x;` guard; helper fns (and the
  `const stdout` line) are only emitted when used.
- Format specs are chosen statically per part (`{d}`/`{s}`/`{}`) — Zig's
  print format is comptime-checked, so each arg's type is known at render
  time (`$x` in interpolation arrives as a `getVar` Call — typed by the
  A2 verdict).
- `--shir-in-zig` reports the ingress marker ("ShIR JSON ingress") on stderr
  and exits 0 for invalid JSON: the backend gate probes the flag by feeding
  intentionally-invalid JSON and grepping the marker, and the harness runs
  under `set -euo pipefail` (a nonzero exit would abort the gate before the
  corpus loop). Renderer panics still exit 101 and fail the corpus loop.

## Next steps (in order)

1. Native `printf` builtin, `WriteFile` append, `Redirect` (fd/mode table),
   `Function`/`Case` (functions as Zig fns, case as switch).
2. The sh2.* runtime port (harness/sh2-namespace.json) — stubs are fine for
   the first green.
3. A `zig build-exe` step in the gate once a Zig toolchain is available to
   the harness (none is installed on the worker today; the emitted code
   targets 0.13+ idioms).

Merge discipline:
- commit on backend/zig; merge main BEFORE each verification run
- push to main only when the commit does NOT touch the shared core
- core changes are single-owner (the estree worker during the lowering
  phase) — queue, don't fork

Verify: the corpus gate (`setup_backends.sh --backend-gate zig`) must stay
100% and the metric must only go down.
