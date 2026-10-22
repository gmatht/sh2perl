# Fish capture bodies — measured grammar constraints

Notes from a failed attempt to fix the multi-command `$( … )` renderings
(baseline: 632 pass / 15 fail on `backends/fish/harness/fish_failmap.sh`).
Every claim below was measured with `fish -n`, fish 3.7.0.

## What a `( … )` body may contain

- `; ` ENDS the enclosing command line. `x=$(a; b)` truncates at the first
  `;` and runs fish's own `exec` for `exec 4>&1` — the help page, then the
  rest of the script leaks into the outer scope. Commands must be separated
  by NEWLINES.
- A bare `(cmd) 2>file` inside `( … )` is "command substitutions not allowed
  here". It must be re-spelled `begin` / cmd / `end`, with the redirect after
  the `end`.
- `end)` on ONE line misparses as the end of an INNER compound ("Missing end
  to balance this begin"). The closing `)` needs its own line.
- `| (` as a pipeline stage: as a STAGE, a group is a run of plain commands
  ending in `end`, so `… | (cmd) 3>&-` must become `… | end; cmd 3>&-` — and
  a stage STARTING with a group must begin `cmd 3>&-; begin`.

## fd dups (`exec 4>&1`) inside a capture

fish has no persistent fd table. A redirect applies:

- to the ONE command it is written on; or
- to the whole SCOPE when it is written on a COMPOUND — `begin … end 4>&1`.

`begin 4>&1` on the `begin` LINE does NOT work: the redirect applies to the
`begin` command itself and is gone at the first newline. The dup must be on an
`end`.

Nesting a `begin` on the line directly after an outer `begin` (with no other
line between) is also rejected by fish's parser, and so is a `begin` block
nested inside a pipeline stage that is itself inside a `begin` block.

## Why this is still open

The shape needed for `parse-dollar-paren-pipe` — a dup scope whose first
command is itself a multi-line compound containing a pipeline — is rejected
by fish in every spelling tried (roughly 30 variants). The capture also
cannot be split into `set tmp (…)` lines: fish ends its command substitution
at the first newline, exactly as it does at `; `.
