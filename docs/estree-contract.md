# ESTree-JSON Contract Norms

Normative guarantees of the `debashc file --estree` output contract
(PLAN.md §1.2). Consumers: the reference executor
(`harness/estree-gen.mjs` + `sh2-namespace.mjs`), sh2runtime (their repo),
and any future backend (C — see `backends/c/docs/backend-c-core-needs.md`,
ask A5). These are the properties a consumer may **rely on** without
re-checking, and the emitter must never violate.

Status: DRAFT.

## 1. Byte strings (raw bytes, non-UTF-8 sources)

Bash strings are **byte strings**; the emitter must preserve them exactly.

- Non-UTF-8 source bytes (>= 0x80) are decoded by the CLI to U+F800+
  private-use chars before parsing (`cli/src/cli_commands.rs`
  `parse_file_to_estree`).
- In emitted JSON strings, those chars are escaped to the marker
  `\x01SH2BYTE\x01<HEX>\x01` (`src/estree.rs` `map_raw_bytes`, const
  `RAW_BYTE_MAGIC`). `<HEX>` is the two-digit uppercase hex byte.
- **The consumer's output path must decode the marker** (the JS runtime's
  `emit` does; a C runtime's `sh2_emit` must too — a ~10-line function).
  A consumer that does not decode it must document that it is lossy on
  non-UTF-8 sources.
- **Embedded NUL:** the emitter must never emit a raw NUL inside a JSON
  string (C consumers would truncate). JSON can carry `\u0000`; the
  structural gate rejects it, or an escape must be defined. This is a
  TODO to enforce in the gate.

## 2. Identifier hygiene

- Loop variables and generated names avoid **JS reserved words**
  (reserved-word-safe loop vars, PLAN.md §7 M4).
- The C backend additionally needs **C keyword avoidance** (`for`, `while`,
  `if`, `int`, `long`, `char`, `static`, `switch`, `case`, `default`,
  `return`, `void`, `sizeof`, ...) — ask A6. Until the emitter mangles,
  C renderers must map (e.g. `if` → `if_`).
- Generated identifiers never collide with the runtime's reserved names
  (`sh2`, `process`, `String`, `Number`, `Math`, `Array`, `Promise`).

## 3. Declaration hoisting

- Variable declarations are emitted at the **top of the program** (all
  `VariableDeclaration` nodes precede use). Consumers may rely on
  declaration-before-use for a two-pass renderer (C needs it: declarations
  at function top).
- Scope: a for-loop iteration variable (`for i in ...`) must not leak into
  the enclosing scope. Loop-scoped declarations are contained.

## 4. Determinism

- Same input → **byte-identical JSON** (mirrors the example-blessing
  determinism workflow, PLAN.md §2.2.4). No iteration counters, no hash
  ordering, no timestamps.

## 5. Structural gate (the properties a green result has)

Validated by `harness/estree_gate.pl`; a green example satisfies all of:

1. **Schema:** every node is standard ESTree.
2. **Callee whitelist:** every `CallExpression` callee is either
   `sh2.<name>` with `<name>` in the `sh2.*` whitelist, `sh2.fs.<name>`
   with `<name> ∈ {readFile, writeFile, appendFile, lstat, unlink, rm,
   mkdir}`, one of the native JS builtins (`String`, `Number`, `parseInt`,
   `parseFloat`, `Promise.all`, `Math.{trunc,floor,ceil,sqrt}`,
   `Number.isNaN`, `Array.isArray`), or a NATIVE_DIRECT_FNS binding called
   via `sh2.callDirect`. The machine-readable spec is
   `harness/sh2-namespace.json` (ask A4).
3. **No `sh2.unsupported`** calls.
4. **No `eval` / `Function` / dynamic import.**
5. **No `*Sync` calls** except the whitelisted pure-CPU loop twins
   (`forLoopSync`, `whileLoopSync`, `cstyleForSync`) — async-only codegen,
   top-level `await`, ESM (`sourceType: "module"`).
6. **Redirect-mode check** (heredoc/herestring shape is exact).

The `sh2.*` whitelist must be kept in sync across: the emitter
(`src/estree.rs` + `src/shir.rs`), the gate (`harness/estree_gate.pl`),
the runtime (`harness/sh2-namespace.mjs`), and the spec
(`harness/sh2-namespace.json`). Any new `sh2.*` name lands in all four or
the gate fails.

## 6. Exit-code / `$?` register

- `sh2.lastExit` holds the last command's exit status; reads are member
  accesses, writes go through `sh2.setLastExit` / assignment.
- A statement that ran a command must have updated `lastExit` before the
  next `$?` read (emitter guarantees ordering).
