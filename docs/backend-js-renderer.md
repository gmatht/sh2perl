# JS backend (backend/js) — renderer contract

Worktree-local JS backend. The renderer lives in `src/js_backend.rs`
(`pub fn shir_to_js(&IrProgram) -> String`), wired into the CLI as
`--shir-in-js` (stdin `-` or a file of ShIR JSON → ESTree JSON on stdout),
mirroring the `--shir-in-estree` arm. The gate probes the flag with the
"ShIR JSON ingress" marker, then renders the shared corpus.

## The contract: estree→js, not a hand-rolled JS printer

`shir_to_js` delegates to the core's `shir::shir_to_estree_json` — the
output is the ESTree JSON data contract (PLAN §1.2), byte-identical to
`--shir-in-estree`. JS text is produced OUTSIDE sh2perl by the harness:

- `harness/estree-gen.mjs` — ESTree JSON → JS text (astring + the
  `lower.js` optimization passes, both from sh2runtime).
- `harness/estree-runner.mjs` — runs the generated JS under node with the
  real `sh2.*` runtime namespace (`harness/sh2-namespace.mjs`).

The backend gate (`setup_backends.sh --backend-gate js`) executes the
emitted JSON through estree-runner.mjs (`--source <file>` so `$0` matches
what `bash <file>` sees) and diffs stdout against bash — the same
execution path as the estree corpus gate (`fail-estree`).

## Why this replaced the original draft

The first `shir_to_js` rendered a small native subset (echo/printf,
assignment, if/loops, simple tests, arith) and emitted compile-able
`sh2_*()` stubs (`console.error` + `process.exit(2)`) or
`/* TODO(unsupported) */` markers for everything else. That draft was
honestly measured at 55/614 corpus files executing correctly — the
runtime port it was waiting for already exists in the harness, and the
worktree now uses it directly.

## Gotchas

- The emitted JSON is the A1 ESTree contract: shell semantics are
  `sh2.*` namespace calls, NOT native JS — do not grep the output for
  `sh2.*` as if it were stubs (the stub gate is disabled for js).
- `$0` semantics: the gate passes `--source <file>` to estree-runner.mjs
  so argv0-based output agrees with bash.
- Keep `shir_to_js` total: a serialization error emits a valid-JSON
  non-ESTree object so the executor fails loudly.

## Next steps

- Nothing to port — the sh2.* runtime is the harness's
  `sh2-namespace.mjs`. The js backend's remaining work is identical to
  the estree backend's: lower `sh2.*` call sites / grow the whitelist
  (metric: `fail-estree --metric`), all single-owner in the shared core.
