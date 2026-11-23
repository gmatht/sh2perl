# The fs/stat command family — what is native, and what still shells out

The census below is the audit trail for the plan's class 5 (command emulation)
and class 6 (stat/identity predicates). It was produced by scanning every
shell-out call site (`__sh_spawn(`, `__sh_run(`, `__sh_shell(`,
`__sh_capture_rc(`) in the 548-render corpus (`555 *.sh` − 7 that fail
`bash -n`), taking the first word of each command, and grouping by word.
Re-measure with:

```
otranspilerl/target/debug/otranspilerl-cli --target rs <example>   # per file
```

At the time of writing the corpus had **505 shell-out call sites in 190
programs** across 64 distinct words. Only the words below are class 5/6; the
rest (grep, sort, sed, awk, perl, diff, cmp, wc, …) are the text-tool classes
owned by later sections.

| word | sites | programs | status |
|---|---|---|---|
| `mkdir` | 0 | 0 | **native** — `FsCommand` node → `std::fs::create_dir`/`create_dir_all` (flags `-p`, `-v` modelled) |
| `touch` | 0 | 0 | **native** — `FsCommand` node → create/append open, mtime bump |
| `sleep` | 0 | 0 | **native** — `std::thread::sleep` in the statement and capture arms |
| `mktemp` | 0 | 0 | **native** — TMPDIR + trailing-X template + `create_new`/`create_dir` uniqueness loop |
| `test`/`[`/`[[` | 0 | 0 | **native** — `FsSameFile`/`FsNewerThan` and the `Path::is_file`/`is_dir`/`exists`/`symlink_metadata` predicate arms |
| `rm` | 5 | 5 | 2 native-eligible shapes blocked by a redirect; 2 globs; 1 unmodelled flag — see below |
| `rmdir` | 1 | 1 | **native** since this pass (`std::fs::remove_dir`); `-p` and `--ignore-fail-on-non-empty` refused |
| `cat` | 22 | 17 | statement reads native (`__sh_cat_stmt`, `__sh_catf`, both `std::fs::read`); pipeline/heredoc/redirect forms keep the shell — see below |
| `cp` | 3 | 3 | **refused**: destination-directory rule + mode bits |
| `mv` | 3 | 3 | **refused**: `rename` is not `mv` (cross-device, existing-target rules) |
| `ls` | 25 | 16 | **refused**: column formatting, `-A`/`-1`/`-l` quoting, sort order |
| `ln` | 2 | 2 | **refused**: not in this family (symlink semantics belong to a link node) |
| `find` | 7 | 6 | **refused**: expression language, not a `std` call |
| `stat` | 3 | 3 | **refused**: format language, not a `std` call |

## The two routes a class-5/6 word can take

1. **The shared shIR node family** — `FsCommand` (`src/shir_nodes/fs_command.node`,
   theorem `FsCommands#command_sound`) for `rm`/`mkdir`/`touch`/`rmdir`, plus
   `FsSameFile`/`FsNewerThan`/`FsModifiedSinceRead` for the stat predicates.
   The node's producer (`src/transforms/fs_cmd_lift.rs`) refuses everything
   the renderer cannot spell: a non-literal word (a variable, a substitution,
   a glob), an unmodelled flag, or no operand at all.
2. **A native statement/capture arm in the renderer** — `cat`,
   `basename`/`dirname`, `mktemp`, `sleep`, `cp`? no: those words whose
   position is not "run this command for its side effect" (`native_cat_stmt`,
   `native_capture_value`). `rmdir` joined this family as a renderer arm: it
   has no statement-position CALL form in this corpus, and giving it a node
   would require extending the `FsCommands` theory's command set.

Either way the command word disappears from the render; anything that keeps
the shell has a recorded reason **in the code**, not only here.

One leniency is worth stating: the NODE producer refuses a runtime word, but
the statement renderers (`exec_stmt` and the `IrStmt::Exec` hook) pass a
variable operand through as ONE path — the same call `native_cat_stmt`
documents for `cat "$f"`. A dynamic operand that expands to several paths or
to a glob (e.g. `rm "$pattern"` with `pattern='*.txt'`) is therefore not
modelled by the native arms; the corpus's dynamic fs operands are single
paths, and refusing them all would only trade the `std::fs` loop for
`bash -c` on every `rm -f "$tmp"` in the corpus.

## Per-word refusals, and where the reason lives

### `rm` — 5 sites

* `rm -r -f DIR 2>/dev/null` (2 sites, `000__03_file_manipulation_commands`,
  `000__04e_file_manipulation`): the flags and the operand are inside the
  modelled subset, but the statement carries a **redirect**. The native loop
  prints bash's diagnostics to stderr unconditionally and does not model
  `2>`; lifting it would leak `rm: cannot remove …` where bash discards it.
  Reason recorded in `rust_backend.rs` at the `IrStmt::Exec` native hook.
* `rm file_*.txt` (2 sites, `011_brace_expansion`,
  `035_brace_expansion_practical`): a glob is ONE `Str` at IR time and N
  paths at runtime — refused by `fs_cmd_lift::plain_word`, documented in
  `fs_command.node` ("a word that is not a plain literal").
* `rm --preserve-root testfile` (1 site, `checkqx-qx-var-rm`): an unmodelled
  flag — refused by `fs_cmd_lift::flags_ok`.

### `cat` — 22 sites

* Statement reads (`cat FILE` with static or single-path words) are native and
  emit `std::fs::read` (`__sh_cat_stmt` / `__sh_catf`); they are not spawns.
* **Pipelines** (`cat file | sort|head|tr …`, 10 sites): the pipeline is
  rendered as one shell command string, so a stage cannot be replaced
  individually; a `cat`-as-producer shortcut would have to re-create the
  pipeline runner's buffer contract. Reason recorded next to
  `native_cat_stmt`.
* **Heredoc writes** (`cat > FILE <<'EOF'`, 7 sites): this is a truncating
  file WRITE with a literal body, not `cat`'s read path; `std::fs::write`
  would be a different operation with different diagnostics and no `-`
  handling. `fs_command.node` lists it under deliberate non-coverage.
* **`cat $tmpf 2>/dev/null`** (2 sites): redirect again (same reason as the
  `rm` redirect rows).
* **`cat -` / bare `cat`** (3 sites): an stdin→stdout byte copy; the statement
  arm refuses an empty word list and a `-` operand because `bash` also
  supports `-` interleaved with files (`fs_cmd_lift` note in
  `native_cat_stmt`).

### `cp` — 3 sites

`cp a b` is not `std::fs::copy`: when `b` is an existing DIRECTORY bash copies
to `b/<basename of a>` (Rust's copy fails), `cp` on a directory source fails
with `cp: -r not specified; omitting directory 'a'`, and the mode of the
destination follows coreutils' rules (`fs::copy` copies the permission bits,
which coincides for these three sites but not in general). The corpus's three
sites (`000__03`, `000__04e`, `builtin-system-open3`) are the plain 2-operand
form, so the destination-directory rule is live runtime state the renderer
cannot see. They keep the spawn; the reason is recorded at
`native_capture_value`'s refusal comment.

### `mv` — 3 sites

`mv` is `rename` only on the same filesystem: across devices it copies and
unlinks, onto an existing file it replaces, onto an existing directory it
moves INTO it, and `mv -Z` (one corpus site) is SELinux context, which `std`
cannot express. No `mv` site is lifted; `std::fs::rename` is refused for the
same reason `cp` is.

### `ls` — 25 sites

Column layout (single vs multi-column depends on the terminal width `ls`
sees), `-1`/`-A`/`-l` quoting, locale collation and `--color` make `ls` an
output formatter, not a directory read. The 25 sites are pipelines
(`ls | grep`, `ls | wc -l`), captures (`ls -A`) and direct statements; none is
lowered. `Path::read_dir` is deliberately NOT used as a stand-in: it would
produce a different byte stream (order, columns, quoting).
