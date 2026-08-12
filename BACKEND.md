# js backend (worktree: /home/llm/sh2loop/sh2perl/backends/js, branch: backend/js)

Shared core (do NOT fork): src/shir.rs (ShIR + lowering), src/ir.rs,
src/estree.rs (node model), src/parser/. Consume the ShIR; render it in
your language's idioms.

Yours (in THIS worktree): the renderer, the corpus gate, and the
sh2.*-usage metric for js. The renderer (`--shir-in-js`) emits the
ESTree JSON contract (delegating to the core's `shir_to_estree_json`);
JS text + the sh2.* runtime are the harness's estree→js implementation
(`estree-gen.mjs` / `estree-runner.mjs` / `sh2-namespace.mjs`).

Merge discipline:
- commit on backend/js; merge main BEFORE each verification run
- push to main only when the commit does NOT touch the shared core
- core changes are single-owner (the estree worker during the lowering
  phase) — queue, don't fork

Verify: the corpus gate must stay 100% and the metric must only go down.
