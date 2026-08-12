# examples.bad/ — corpus examples the stdout-vs-bash gate should NOT measure

Moved out of examples/ (the `fail` glob) because their reference output
cannot verify a faithful translation — see BAD_EXAMPLES.md (workspace
root) for the full analysis and the per-category tables.

- **deliberate-syntax-errors/** — bash-rejected scripts (exit 2, no
  stdout). The only useful property is "the parser must not crash on bad
  input", which is a STRUCTURAL check (harness/check_ast.pl style), not a
  stdout diff. Moving them does not remove that purpose — it stops the
  stdout gate from mis-measuring it (a translation that prints nothing
  "passes", one that prints anything "fails", regardless of merit).
- **bash-runtime-state/** — the reference output is bash reporting on
  itself (`$-` flags, `${BASH_VERSION}`, tty device paths). A translated
  program is not a bash process; matching means fabricating constants
  that are wrong for any other invocation. No faithful translation can
  produce it — no script fix helps.

Each file keeps its history (git mv) and its content unchanged.
