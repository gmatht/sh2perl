# JS backend (backend/js) — renderer contract

Worktree-local JS backend. The renderer lives in `src/js_backend.rs`
(`pub fn shir_to_js(&IrProgram) -> String`), wired into the CLI as
`--shir-in-js` (stdin `-` or a file of ShIR JSON → JS source on stdout),
mirroring the `--shir-in-perl` arm. The gate probes the flag with the
"ShIR JSON ingress" marker, then renders the shared corpus.

## Lowable subset (native lowering)

- Output / `exec echo` → `process.stdout.write(...)` / `console.log(...)`
- Assign / Declare (with A2 `var_types` coercion: `Int` → `expr_as_num`,
  numeric `Str("5")` literals parsed; `Str`/Any → as-is)
- DeclareArray → `name = [...];`
- If / For (`for (let i of ...)`) / While / DoWhile / Block
- Exit → `process.exit(...)`; Return (inside `main()` / subs)
- Functions (`IrStmt::Function`, `IrSub`) → `function ...`
- `getVar` on typed vars → bare identifier; `test` mini-evaluator
  (`-gt -lt -ge -le -eq -ne -n -z`, `=`, `==`, `!=`, single operand)
- Arith: native JS with `Number(x) || 0` coercion on non-Int vars
  (`**` → `Math.pow`; `Assign`/`IncDec` → `sh2.arith` stub)
- BinOp: `Not` is unary (rhs is a parser duplication — ignore it)

Everything else emits a compile-able `sh2_*()` stub or a
`/* TODO(unsupported) */` marker (sanitized against `*/`). All 535
corpus renders pass `node --check`.

## Gotchas learned

- Identifiers are mangled against JS reserved words (`js_ident`).
- Indexed assign targets arrive as whole names (`map[foo]`) — hoist the
  base name as `{}` so index writes are valid JS.
- Vars are hoisted as `let` at the top of `main()` (mirrors the C
  renderer), so `return` at top level stays valid.
- `main()` is invoked at the end; the program is a Node script
  (`#!/usr/bin/env node`, `"use strict"`).

## Next steps (in order)

1. Runtime: port the `sh2.*` namespace (see
   `backends/c/docs/backend-c-core-needs.md` §7 for the per-language
   table; `harness/sh2-namespace.json` is the spec). Stubs currently
   `console.error` + `process.exit(2)`.
2. Grow the native subset: `param`/`join`/`slice` echo args, `setArray`,
   `WriteFile`, `Case`, `Redirect` (native shell-lexing helpers), then
   `Pipeline`/`Subshell`/`Background`.
3. A per-language sh2.*-usage metric (mirror `harness/sh2stat.pl`).
