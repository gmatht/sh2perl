# ideom/ — Idiom review workflow

## Purpose

For each shell script that successfully translates to Perl (via both
`./fail` for examples/ and `check_sh_files.pl` for sh/), generate an
idiom review `.md` file in `ideom/` that:

1. Evaluates how idiomatic the generated Perl is
2. Identifies patterns that an optimizing IR backend could fix
3. Drives generator improvements by migrating those patterns to the
   Perl IR (`src/ir.rs`, see `docs/ir-design.md`)

## Directory structure

```
ideom/
  examples/           # reviews for examples/*.sh
    001_simple.sh.md
    002_control_flow.sh.md
    ...
  sh/                 # reviews for sh/* files
    VampireDriversFunctions.sh.md
    adduser.preinst.sh.md
    ...
```

Each `.md` file follows a standard template (see below).

## Workflow

### Trigger

Run `scripts/next-ideom-review` (or similar) after `./fail` and
`check_sh_files.pl` both pass.  The script:

1. Finds the first script in `examples/` or `sh/` that doesn't have
   a corresponding `.md` in `ideom/`.
2. Generates the Perl code with `debashc -i <file> -o /dev/stdout`.
3. Invokes pi with the template below to produce the review.
4. Saves the review to `ideom/{examples|sh}/<filename>.md`.

### Review template

Each review answers:

```markdown
# Idiom review: examples/foo.sh

## Source
```bash
<original shell script>
```

## Generated Perl
```perl
<current generated code>
```

## Idiom issues

| # | Pattern | Generated code | Idiomatic Perl | IR-fixable? |
|---|---------|---------------|----------------|-------------|
| 1 | ... | ... | ... | Yes/No |

## IR-fixability

For each issue marked "IR-fixable: Yes", describe:
- Which IR node(s) would replace the current text emission
- How `ir_to_perl()` would produce cleaner output from that node
- What the generated code would look like after the fix

## Optimizations applied

After fixing an issue via the IR, update this section:
- IR node added: `IrStmt::Foo`
- Old output: `...`
- New output: `...`
- Commit: `<hash>`
```

### Driving generator improvements

When a review identifies an IR-fixable pattern:

1. Extend `src/ir.rs` with the necessary IR node (if not yet present).
2. Migrate the relevant `generate_*` function from emitting `format!()`
   to returning `IrStmt::Foo(...)`.
3. Adjust `ir_to_perl()` in `src/ir.rs` to emit clean Perl from that node.
4. Verify with `./fail` and `check_sh_files.pl`.
5. Commit with message referencing the review file, e.g.:
   `"IR: migrate echo generation to IrStmt::Output (ref ideom/foo.md)"`

### Prioritisation

Order of reviews (by expected impact):

1. **examples/ files** — simpler, shorter, more focused
2. **sh/ files that currently pass** — real-world code, higher value
3. **sh/ files that fail** — fix the generator first, then review

## Next steps

1. Create `ideom/` directories
2. Write `scripts/next-ideom-review` script
3. Run first review on `examples/001_simple.sh`
4. Implement first IR migration based on review findings
