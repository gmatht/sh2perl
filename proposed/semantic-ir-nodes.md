# Semantic IR Nodes for String/Text Operations

## Motivation

Shell commands like `cut`, `tr`, `sed`, `head`, `wc` operate on text in
well-understood ways. Currently:

- **ESTree path**: `cut -d',' -f2` is lowered to JS AST (`split`/`filter`/`join`
  chains) in `shir.rs` — hardcoded to one backend.
- **Perl path**: stays as `Exec { cmd: "cut", ... }` or `sh2.cutText(...)` — no
  native inlining.
- **Future backends (C, Rust, Go)**: would need to re-implement the same
  decomposition independently.

The problem: the *intent* ("extract field 2 using comma delimiter") is lost
after the ESTree path lowers it. Other backends see only `sh2.cutText(...)` —
a black-box runtime call.

## Proposal

Add **semantic IR nodes** that capture the *meaning* of text operations.
The shared pass lowers shell commands to these nodes; each backend renders
them with native idioms or falls back to `sh2.*` calls.

```
Shell:  echo "$csv" | cut -d',' -f2
         ↓ pattern lift
shIR:   FieldExtract { text: $csv, delimiter: ',', fields: [1] }
         ↓ backend
Perl:   (split(',', $csv))[1]
JS:     csv.split(',')[1]
C:      strtok / memchr loop
Rust:   csv.split(',').nth(1)
```

## New IR Nodes

### String Operation Expressions

These are `IrExpr` variants — they evaluate to a value.

```rust
/// Extract fields from delimited text (lowered from `cut -dD -fF`).
///
/// `delimiter` is the field separator character (single char for `-d`).
/// `fields` is the 1-indexed list of fields to keep (merged ranges
/// like `1-3,5` are expanded). When `suppress_no_delim` is true (`-s`),
/// lines without the delimiter are omitted from the result entirely.
///
/// Backends:
///   Perl:  split(delimiter, text, -1)[fields] with join
///   JS:    text.split(delimiter).filter(...).join(delimiter)
///   C:     strtok_r loop
///   Rust:  text.split(delimiter).skip(...).take(...)
///   Fallback: sh2.fieldExtract(text, delimiter, fields, suppress_no_delim)
FieldExtract {
    text: Box<IrExpr>,
    delimiter: String,           // single char, e.g. ","
    fields: Vec<FieldRange>,     // 1-indexed ranges, e.g. [1, 3..5]
    suppress_no_delim: bool,     // -s flag
    output_delimiter: Option<String>,  // -d output delim (None = same as input)
}

/// Extract characters by position (lowered from `cut -cN` or `cut -bN`).
///
/// `positions` is 1-indexed code-point positions or byte positions.
/// The `byte_mode` flag distinguishes `-b` from `-c` (relevant for
/// multi-byte encodings; most backends treat them identically).
CharExtract {
    text: Box<IrExpr>,
    positions: Vec<FieldRange>,
    byte_mode: bool,             // true = -b, false = -c
}

/// Translate characters (lowered from `tr SET1 SET2`).
///
/// `from` and `to` are the character sets. When `delete` is true
/// (`tr -d SET1`), characters in `from` are removed (ignores `to`).
/// When `squeeze` is true (`tr -s SET1`), runs of the same character
/// in `from` are collapsed to one.
///
/// Backends:
///   Perl:  $text =~ tr/abc/xyz/d  (or tr/abc//d for delete)
///   JS:    replace regex or lookup table
///   C:     translate table or ctype
///   Rust:  text.chars().map(|c| table.get(&c).unwrap_or(c))
///   Fallback: sh2.translate(text, from, to, delete, squeeze)
CharTranslate {
    text: Box<IrExpr>,
    from: String,
    to: String,                  // empty when delete=true
    delete: bool,                // tr -d
    squeeze: bool,               // tr -s
}

/// Regex substitution (lowered from `sed 's/pattern/repl/'`).
///
/// `pattern` is a regex pattern (BRE or ERE, depending on flags).
/// `replacement` is the replacement string (with back-references).
/// `global` is true for `s/p/r/g`. `line_mode` is true when operating
/// on each line (`sed` default) vs the whole input.
///
/// Backends:
///   Perl:  $text =~ s/pattern/repl/g
///   JS:    text.replace(/pattern/g, repl)
///   C:     regex.h or PCRE
///   Rust:  regex::Regex::replace
///   Fallback: sh2.regSub(text, pattern, replacement, global)
RegSub {
    text: Box<IrExpr>,
    pattern: String,
    replacement: String,
    global: bool,
    line_mode: bool,             // per-line vs whole-input
}

/// Substring extraction (lowered from `cut -cN-M`, `expr substr`, etc.)
///
/// `offset` is 0-indexed start position. `length` is the number of
/// characters (None = rest of string).
SubStr {
    text: Box<IrExpr>,
    offset: Box<IrExpr>,
    length: Option<Box<IrExpr>>,
}

/// String length (lowered from `${#var}`).
StrLen {
    text: Box<IrExpr>,
}

/// Take first/last N lines (lowered from `head -n N`, `tail -n N`).
///
/// `count` is the number of lines. `from_end` is true for `tail`.
/// When `bytes` is true, count is in bytes not lines (`head -c`, `tail -c`).
TakeLines {
    text: Box<IrExpr>,
    count: Box<IrExpr>,
    from_end: bool,              // tail vs head
    bytes: bool,                 // -c vs -n
}

/// Count lines, words, bytes, or characters (lowered from `wc`).
///
/// `mode` is 'l' (lines), 'w' (words), 'c' (bytes), 'm' (chars).
/// `text` is the input. Returns an integer.
///
/// Backends:
///   Perl:  scalar(split(/\n/, $text)) for -l
///   JS:    text.split('\n').length for -l
///   Rust:  text.lines().count()
///   Fallback: sh2.wc(text, mode)
WordCount {
    text: Box<IrExpr>,
    mode: char,                  // 'l', 'w', 'c', 'm'
}

/// Lowercase/uppercase transformation (lowered from `tr 'A-Z' 'a-z'`,
/// `declare -l var`, `${var,,}`, `${var^^}`).
///
/// `upper` is true for uppercase, false for lowercase.
/// This is a common special case of CharTranslate that backends
/// can render with native idioms.
CaseTransform {
    text: Box<IrExpr>,
    upper: bool,
}

/// String contains test (lowered from `grep -q`, `case *P*)`, etc.)
/// Already exists as sh2.contains — this makes it a first-class IR node.
///
/// Backends:
///   JS:    text.includes(pattern)
///   Perl:  index($text, $pattern) != -1
///   Rust:  text.contains(pattern)
///   C:     strstr(text, pattern) != NULL
StringContains {
    text: Box<IrExpr>,
    pattern: Box<IrExpr>,
}

/// String starts/ends with test (lowered from `case P*)`, `[[ $x == P ]]`).
///
/// `prefix` is true for starts-with, false for ends-with.
StringAffix {
    text: Box<IrExpr>,
    pattern: Box<IrExpr>,
    prefix: bool,
}

/// String trim (lowered from `sed 's/^[[:space:]]*//'`, `xargs` leading
/// whitespace removal, etc.).
///
/// `leading` and `trailing` control which side to trim.
StringTrim {
    text: Box<IrExpr>,
    leading: bool,
    trailing: bool,
}

/// Repeat a string N times (lowered from `printf '%100s' '' | tr ' ' C`).
RepeatStr {
    text: Box<IrExpr>,
    count: Box<IrExpr>,
}
```

### Supporting Types

```rust
/// A 1-indexed field range, e.g. `1` = field 1, `3..5` = fields 3-5.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldRange {
    Single(u32),
    Range { start: u32, end: u32 },
}
```

## Shared Pass: Command Recognition → Semantic Nodes

The pattern lift system already exists (`shir_passes/pattern/`). We extend
it with a new module: `text_ops.rs`.

### Recognition Rules

```rust
// In shir_passes/pattern/text_ops.rs

/// Recognize `echo ARGS | cut -dD -fF` and lower to FieldExtract.
pub struct CutFieldExtract;

impl PatternLift for CutFieldExtract {
    fn name(&self) -> &'static str { "cut_field_extract" }
    
    fn try_lift_stmt(&self, stmt: &IrStmt) -> Option<IrStmt> {
        // Match: Pipeline { stages: [echo_stage, cut_stage] }
        // where cut_stage is Exec { cmd: "cut", args: ["-dD", "-fF"] }
        // → FieldExtract { text: echo_args, delimiter: D, fields: F }
        // ...
    }
}

/// Recognize `echo ARGS | cut -cN` and lower to CharExtract.
pub struct CutCharExtract;

/// Recognize `echo ARGS | tr FROM TO` and lower to CharTranslate.
pub struct TrCharTranslate;

/// Recognize `echo ARGS | sed 's/P/R/'` and lower to RegSub.
pub struct SedRegSub;

/// Recognize `echo ARGS | head -n N` and lower to TakeLines.
pub struct HeadTakeLines;

/// Recognize `echo ARGS | tail -n N` and lower to TakeLines.
pub struct TailTakeLines;

/// Recognize `echo ARGS | wc -l` and lower to WordCount.
pub struct WcWordCount;
```

### Example: CutFieldExtract Recognition

```rust
fn try_lift_stmt(&self, stmt: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Pipeline { stages, .. } = stmt else { return None; };
    if stages.len() != 2 { return None; }
    
    // Stage 1: echo (or printf, or a known text producer)
    let text_expr = match stages[0].as_slice() {
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec" && is_echo_call(args) =>
        {
            // Reconstruct the text expression from echo args
            rebuild_echo_text(args)?
        }
        _ => return None,
    };
    
    // Stage 2: cut with static args
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stages[1].as_slice() 
        else { return None; };
    if func != "exec" || !is_cut_call(args) { return None; }
    
    let cut_args = extract_args(args)?;
    let spec = parse_cut_spec(&cut_args)?;  // -d, -f, -s, -c
    
    match spec.mode {
        'f' => Some(IrStmt::Expr(IrExpr::FieldExtract {
            text: Box::new(text_expr),
            delimiter: spec.delimiter,
            fields: spec.fields,
            suppress_no_delim: spec.suppress,
            output_delimiter: spec.output_delim,
        })),
        'c' | 'b' => Some(IrStmt::Expr(IrExpr::CharExtract {
            text: Box::new(text_expr),
            positions: spec.fields,
            byte_mode: spec.mode == 'b',
        })),
        _ => None,
    }
}
```

## Backend Rendering

### Perl: `ir_to_perl`

```rust
IrExpr::FieldExtract { text, delimiter, fields, suppress_no_delim, output_delimiter }
    if output_delimiter.is_none() && !suppress_no_delim =>
{
    // Simple case: split + index
    let idx_list: Vec<_> = fields.iter().map(|f| match f {
        FieldRange::Single(n) => n - 1,  // 0-index for Perl
        _ => return None, // complex case falls back
    }).collect();
    if idx_list.len() == 1 {
        format!("(split({}, {}, -1))[{}]", 
            quote_perl(&delimiter), ir_expr_to_perl(text), idx_list[0])
    } else {
        // Multiple fields: split + join
        let indices: Vec<_> = idx_list.into_iter()
            .map(|i| format!("$_[{}]", i))
            .collect();
        format!("join({}, (split({}, {}, -1))[{}])",
            quote_perl(&output_delimiter.as_ref().unwrap_or(&delimiter)),
            quote_perl(&delimiter), ir_expr_to_perl(text),
            indices.join(", "))
    }
}
// Complex case: falls back to sh2.fieldExtract(...)
_ => format!("sh2.fieldExtract({}, {}, {}, {}, {})",
    ir_expr_to_perl(&text), quote(&delimiter),
    fields_to_perl(fields), suppress_no_delim,
    output_delimiter.as_ref().map(|d| quote(d)).unwrap_or("null".into()))
```

### JS/ESTree: `shir_to_estree`

```rust
IrExpr::FieldExtract { text, delimiter, fields, suppress_no_delim, output_delimiter }
    if output_delimiter.is_none() && !suppress_no_delim =>
{
    // Native JS: split + filter by index + join
    let text_js = ir_expr_to_estree(text)?;
    let delim_js = ir_expr_to_estree(&IrExpr::Str(delimiter.clone(), StrStyle::SingleQuoted))?;
    
    // text.split(delimiter)
    let split = call(method(text_js, "split"), vec![delim_js.clone()]);
    
    // .filter((_, i) => indices.includes(i + 1))
    let indices = fields.iter().map(|f| match f {
        FieldRange::Single(n) => *n as i64,
        FieldRange::Range { start, end } => (*start as i64)..=(*end as i64),
    }).flatten().collect::<Vec<_>>();
    let i = ident("_i");
    let pred = arrow_func(
        vec![underscore(), i.clone()],
        includes_lit(i.clone(), indices),
    );
    let filtered = call(method(split, "filter"), vec![pred]);
    
    // .join(delimiter)
    call(method(filtered, "join"), vec![delim_js])
}
// Complex: sh2.fieldExtract(...)
_ => sh2_call("fieldExtract", vec![text, delimiter, fields, suppress, out_delim])
```

### C: `shir_to_c`

```rust
IrExpr::FieldExtract { text, delimiter, suppress_no_delim, .. } => {
    // Emit a strtok_r loop
    let c_text = ir_expr_to_c(text)?;
    let c_delim = ir_expr_to_c(&IrExpr::Str(delimiter.clone(), StrStyle::SingleQuoted))?;
    
    // Generated C code:
    // char *saveptr;
    // char *tok = strtok_r(buf, delim, &saveptr);
    // while (tok) { /* collect field if in list */ tok = strtok_r(NULL, delim, &saveptr); }
    c_strtok_extract(c_text, c_delim, fields)
}
```

## Registration

In `shir_passes/pattern/mod.rs`:

```rust
pub mod text_ops;

pub fn all_pattern_lifts() -> Vec<Box<dyn PatternLift>> {
    vec![
        Box::new(contains::GrepTest),
        Box::new(contains::CaseGlob),
        Box::new(contains::TestGlob),
        // New text operations
        Box::new(text_ops::CutFieldExtract),
        Box::new(text_ops::CutCharExtract),
        Box::new(text_ops::TrCharTranslate),
        Box::new(text_ops::SedRegSub),
        Box::new(text_ops::HeadTakeLines),
        Box::new(text_ops::TailTakeLines),
        Box::new(text_ops::WcWordCount),
        Box::new(text_ops::StringContainsNode),
        Box::new(text_ops::CaseTransformNode),
    ]
}
```

## Migration Path

1. **Phase 1**: Add the IR nodes to `IrExpr` (additive, no existing code breaks).
2. **Phase 2**: Implement the pattern lifts in `text_ops.rs` (corpus-gated).
3. **Phase 3**: Add renderers in each backend (Perl first, then ESTree, then C).
4. **Phase 4**: Remove the existing JS-AST `cut` lowering from `shir.rs`
   (replaced by the semantic IR node → ESTree renderer).

Each phase is independently testable and corpus-gated.

## What Stays as sh2.* Calls

Not everything decomposes into semantic nodes. These stay as runtime calls:

- `sh2.pipeline(...)` — arbitrary pipe chains with unknown commands
- `sh2.exec(...)` — unknown external commands
- `sh2.subshell(...)` — copy semantics
- `sh2.background(...)` — async execution
- `sh2.redirect(...)` — complex fd manipulation
- `sh2.builtin(...)` — builtins with side effects (cd, export, etc.)
- `sh2.test(...)` — the full test expression parser (too complex to decompose)

The semantic nodes are for **text transformations** — operations that take
text in and produce text out, with well-known idioms in every language.
