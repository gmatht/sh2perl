# Failing Test Notes

## Backend gate (shIR renderer) — current state (2026-08-15, final)

Gate: `setup_backends.sh --backend-gate perl` — **626/627, 0 stubs, 0
skip** (render is stub-free for the whole corpus). The remaining single
failure is CORE-BLOCKED:

- `utf8-non-utf8-content.sh` — the CORE's top-level `--shir`/`--shir-raw`
  CLI arm still decodes invalid-UTF-8 bytes lossily (U+FFFD); the gate's
  emit path is the top-level arm, so the A1 JSON carries the replacement
  char and no renderer can reproduce byte 0xE9. The `file --shir`
  subcommand already emits the PUA marker (U+E000+byte, the
  perl-20260814-175710 convention) and the worktree renderer decodes it
  byte-exactly (verified). Pending:
  `core-requests/sh-20260815-115501-utf8-toplevel-shir-arm.md` (the estree
  worker must switch the top-level arm to the marker decode — a 1-line
  core change). Once it lands, the gate reads 627/627.

## Session history

- 2026-08-14 first session: synced the branch with main (164 commits —
  Ident arith, Sizeof/Cast, named_blocks, core passes), fixed the test
  evaluator (tokenizer quote stops, fused parens/ops, chomped cmdsubs),
  capture reconstruction (subshell/block/case/return/ForInit arms,
  redirect Object-spec form, fd dups/closes, arith-in-shell, qx escaping
  guards), local/index/assoc registration, eval/trap/source builtins,
  non-literal exec + bash fallback, sh2-split in emitted strings.
- 2026-08-14 second session: 431→612/613 (0 stubs) — A1 source-field $0,
  PIPESTATUS, clobber redirects, case/glob ordering, eval'd functions,
  procsub fifos, and the gate's private-scratch + exit-code-match
  semantics (setup_backends.sh).
- 2026-08-15: 612→626/627 — signed `~` arith (use integer), eval
  plain-assignment runtime (our-hoisted), source file_writes native
  assignment, printf %b unescape (fmt literal normalization), sh2-split
  valid-perl concat, rm/unlink status propagation + glob() runtime
  expansion, trailing exit after status-tracking unlink.
