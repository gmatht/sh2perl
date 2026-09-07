# Release binary size

Measured on `otranspiler` (the default user-facing binary, package `otranspilerl`,
lib `sh2perl core`), rustc 1.96.1, x86_64-unknown-linux-gnu.

## TL;DR

| Build | Size | Profile |
|---|---|---|
| `target/debug/otranspiler` | **93.5 MB** | cargo defaults: no opt, full DWARF, no strip, no LTO |
| `target/release/otranspiler` (default) | **5.35 MB** | `[profile.release]`: `opt-level="z"`, `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true`; `legacy-generator` feature OFF |
| `target/release/otranspiler` (`--features legacy-generator`) | **6.15 MB** | same profile, legacy AST-side Perl generator linked |

- ~67 % of the debug binary is **debug info**, not code. The actual program is ~6 MB.
- The release binary is ~95 % real content: 4.2 MB code, 714 KB rodata, 637 KB
  `.eh_frame`, 284 KB data.
- The 637 KB `.eh_frame` (panic/unwind tables) is **not needed functionally**
  (`panic = "abort"` is already set; the shipped binary never catches panics),
  but it is **not removable via Rust flags** — it is std's precompiled unwind
  tables (measured; see below).

## Debug build: what is in the 93.5 MB

The dev profile ships full DWARF and zero optimization, so the size is dominated
by metadata:

| Section | Size | What it is |
|---|---|---|
| `.debug_info` | 24.8 MB | DWARF type/function info |
| `.debug_str` | 22.1 MB | DWARF string table |
| `.debug_line` | 8.0 MB | line tables |
| `.debug_ranges` | 5.7 MB | address ranges |
| `.debug_loc` / `.debug_aranges` / `.debug_abbrev` | ~2.1 MB | misc DWARF |
| `.text` | 18.5 MB | actual code (17.6 MiB per `cargo bloat`) |
| `.rela.dyn` | 1.5 MB | dynamic relocations |
| `.eh_frame` | 1.2 MB | unwind tables |
| `.gcc_except_table` | 881 KB | exception tables |
| `.rodata` | 849 KB | literals |

**Debug info alone is ~63 MB (~67 %).** The code is 17.6 MiB, dominated by big
monomorphized, match-heavy functions (dev build, `cargo bloat`):

- `shir::stmt_to_estree` 177 KB, `expr_to_estree` 116 KB, `try_native_param` 97 KB
- legacy perl generator: `word_to_perl_impl` 101 KB, `generate_simple_command_impl`
  97 KB, `generate_grep_command` 89 KB, `test_expression_impl` 79 KB, pipeline
  generators
- the whole backend fleet linked in (no LTO): `c_backend::Render::stmt` 92 KB,
  `rust_backend` 52 KB, `zig_backend` 47 KB, plus java/python/go/sh/glsl/perl/lint
- A1 JSON (de)serialization: `shir_json::stmt_json`/`expr_json` (65/41 KB),
  `shir_json_in::stmt_from` (55 KB) — serde on the giant `IrStmt`/`IrExpr` enums
- glob/grep engine: `aho_corasick` Teddy AVX2 ×4 SIMD widths (~210 KB) + `regex_automata`
- even the CLI's own `main_with_args` (~100 KB) and `otranspilerl::testing::*` test
  functions (~86 KB)

## Release build: what is in the 6.2 MB

Sections (stripped binary; the unstripped build is 9.2 MB, the ~3 MB delta is
the `.symtab`/`.strtab` that `strip=true` removes):

| Section | Size | What it is |
|---|---|---|
| `.text` | 4.2 MB | actual code |
| `.rodata` | 714 KB | ~50k string literals + embedded per-backend runtime helper source (`def sh2_memStore`, `#!/usr/bin/env perl`, `#define _GNU_SOURCE`, `extern "C"` ×24) |
| `.eh_frame` | 637 KB | unwind tables (see below) |
| `.data` | 284 KB | static initialized data |
| `.rela.dyn` | 327 KB | dynamic relocations |

Code ownership of the 4.3 MB `.text`, by family (measured via `nm`):

| Owner | `.text` | Notes |
|---|---|---|
| `std`/deps | 914 KB | serde_json, backtrace-rs, allocator/panic machinery |
| backend fleet | 900 KB | c_backend 262 KB + go/python/rust/zig/java/sh/glsl/perl/lint 638 KB — only one runs per invocation |
| legacy perl generator | 607 KB | `word_to_perl`, `simple_command`, `generic_builtin`, `grep`, `test_expr`, pipelines — **gated behind the `legacy-generator` feature (off by default)** |
| estree emitters (`shir`) | 533 KB | `stmt_to_estree`, `expr_to_estree`, `try_native_param` |
| `sh2perl core` core | 513 KB | parser, ir, ast_words |
| transforms (passes) | 307 KB | incl. `transform_stmt` |
| regex/glob engine | 305 KB | `regex_automata` + `aho_corasick` |
| `otranspilerl` cli | 126 KB | `main_with_args` 35 KB |
| A1 JSON ser/de | 106 KB | `stmt_json`/`expr_json`/`stmt_from` |

Biggest single functions (release, LTO already collapsed the debug giants —
177 KB `stmt_to_estree` → 51 KB):

`regex_automata::meta::strategy::new` **77.5 KB** (biggest in the binary),
`transforms::shir_native_stmt::transform_stmt` **61 KB**, `stmt_to_estree` 51 KB,
`generate_simple_command_impl` 50 KB, `generate_generic_builtin` 48 KB,
`expr_to_estree` 44 KB, `c_backend::Render::stmt` 40 KB. There is no single
villain; the weight is a long tail of thousands of small functions (sum of
symbol sizes = 4.31 MB ≈ `.text`).

### Structural drivers (why it is this big)

1. **"One library, every target."** `sh2perl core` contains the legacy generator +
   estree emitters + all 10 backends, all reachable from `--target` dispatch, so
   LTO cannot drop them — ~1.5 MB of the 4.2 MB is renderers/generators only one
   of which is used per run.
2. **A1 JSON round-trip is load-bearing.** The otranspiler flow is
   parse → A1 JSON → spawn `--shir-in-<tgt>`, so both serializing and
   deserializing the whole `IrStmt`/`IrExpr`/`ArithAst` enums are always linked
   (106 KB ser/de + serde_json).
3. **Regex/glob engine** (glob matching, `${x#pat}`, grep emulation): ~305 KB.
4. **Strings in rodata**: runtime `sh2.*` names, ~50k literals, and the full
   runtime helper source embedded per backend.

## Panic tables: do we need them?

`.eh_frame` (637 KB, 6909 FDEs) is the DWARF-based unwinding tables used to
propagate panics (run destructors, then abort or catch). The question is whether
this binary needs them.

**Functionally: no.** The shipped `otranspiler` never catches panics — the only
`catch_unwind` in the tree is `src/bin/render_all_nodes.rs:68`, a dev tool, not
the shipped binary. `[profile.release]` already sets `panic = "abort"`, so a
panic prints the message and aborts; no unwinding ever happens. The tables are
dead weight for this binary.

**But they are not removable via Rust flags (measured).** Attempted:

```
RUSTFLAGS="-C force-unwind-tables=no" cargo build --release \
  --config 'profile.release.panic="abort"' -p otranspilerl --bin otranspiler
```

Result: `.eh_frame` **unchanged** (637.6 KB), `.text` unchanged (4278.3 KB),
total unchanged (6.2 MB) — despite a full rebuild and the flag confirmed in the
cargo fingerprint (`"rustflags":["-C","force-unwind-tables=no"]`).

Why: the tables are **std's precompiled unwind tables**. `-C force-unwind-tables=no`
only affects crates compiled locally; std ships precompiled with unwind tables
and they are linked in regardless. Verified on minimal crates (rustc 1.96.1):

```
fn main(){println!("hi")}            # .eh_frame 20.2 KB  (with AND without the flag)
fn big(){...1M-iter loop...}         # .eh_frame 20.4 KB  (with AND without the flag)
```

The flag produces byte-identical `.eh_frame` on this toolchain.

**The only way to reclaim the 637 KB** is a post-link section strip:

```
objcopy --remove-section=.eh_frame --remove-section=.eh_frame_hdr target/release/otranspiler
```

This is safe for `panic = "abort"` (no unwinding ever runs), but it **degrades
panic backtraces** — `backtrace-rs` needs the frame info to walk the stack
(unless the binary is built with frame pointers). For a CLI transpiler that is a
debugging nicety, so the trade is defensible but probably not worth 637 KB of
6.2 MB (~10 %).

## How to measure

```sh
# section breakdown
size -A target/release/otranspiler | sort -k2 -rn | head

# biggest functions (needs an unstripped build: --config 'profile.release.strip=false')
nm -S --size-sort --radix=d target/release/otranspiler | tail -30

# crate/function breakdown (cargo-bloat)
cargo bloat -p otranspilerl --bin otranspiler -n 25

# unwind tables
readelf --debug-dump=frames target/release/otranspiler | grep -c "pc="   # FDE count
```

## Levers (if size ever matters)

- **`legacy-generator` feature (done).** The legacy AST-side Perl generator
  (`src/generator/`, 36k lines) is gated behind a cargo feature, **off by
  default**: the new perl backend (`ir::shir_to_perl`) compiles without it
  (emulated commands fall back to `bash -c` shell-out), and the legacy
  CLI/wasm/wasi `to_perl` utilities degrade to a stub message. Saves ~0.8 MB
  of the release binary (~640 KB of `.text`). Enable with
  `cargo build --features legacy-generator`.
- **Feature-gate the rest of the backend fleet** per `--target` (only compile
  the requested renderer): saves ~0.9 MB of `.text`.
- **Drop `serde_json` from the in-process render path** (call the renderer
  directly instead of A1 JSON round-trip): saves the 106 KB ser/de + part of
  the serde_json dependency.
- **Post-link `.eh_frame` strip** (above): saves 637 KB, costs panic backtraces.
- The debug binary is already fixed by the release profile; nothing to change
  for distribution.
