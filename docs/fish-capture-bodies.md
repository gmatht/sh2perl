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
by fish in every spelling tried (roughly 30 variants).

A `set __t ( … )` carrier instead of the inline body DOES parse (the nesting
that breaks the inline form is fine there), but it does not fix the case:

```fish
set __sh2_t1 (begin
  begin
  echo before >&4
end 4>&1
  begin; begin; echo sub; end 3>- | cat
end 4>&1
begin
  echo after >&4
end 4>&1
end)
# -> [before sub after]      WRONG (bash: before / sub / after, one per line)

# the same body as ONE inline `( … )` -> [before] [sub] [after]  RIGHT
```

So the substitution BOUNDARY is not what loses `after` — the redirect
SCOPING is. `end 4>&1` installs fd 4 only for the compound it closes; the
next sibling command starts with fd 4 closed again. The inline form works only
because the whole body is ONE compound, and one compound containing a
pipeline-stage group is exactly what the parser rejects.

The two requirements are in direct conflict in fish 3.7:
- keep a dup live across sibling commands -> they must share ONE compound;
- a compound may not contain the pipeline-stage group the corpus uses.

Any fix has to change what the renderer emits at the CORE level (e.g. keep
such capture bodies single-command) or accept the lost fidelity for
`exec N>&1` inside `$( … )`.
