# Perl IR — Intermediate Representation for Code Generation

## Motivation

The current generator emits Perl text via `format!()` calls throughout
`src/generator/`. This makes it impossible to:

- Change coding style (e.g. `print $x; if ...` → `say $x`) without editing
  every `generate_*` function
- Optimize the output (dead code elimination, import minimization)
- Support multiple backend languages
- Reason about the generated program at a semantic level

The IR decouples *what* to generate from *how* to format it.

## Architecture for multi-language support

The IR designed here is **language-specific** (Perl).  It's one backend
in a layered architecture that can grow to support other targets:

```
┌──────────┐   common, language-neutral
│ Shell AST │   (already exists)
└─────┬─────┘
      │                  ─── shell-level analyses
      ▼
┌──────────────┐   shared semantic analysis
│ Language-     │   (doesn't exist yet — optional)
│ neutral IR    │   Models: variables, commands, pipelines,
│ (ShIR)        │   control flow, substitutions
└───┬──┬──┬────┘
    │  │  │                ─── language-specific lowering
    ▼  ▼  ▼
┌──────┐ ┌──────┐ ┌──────┐
│Perl   │ │Python│ │Rust  │   language-specific IRs
│IR     │ │IR    │ │IR    │   (this document = Perl IR)
└───┬───┘ └──┬───┘ └──┬───┘
    │        │        │        ─── pretty-print
    ▼        ▼        ▼
  perl.pl   py.py    rs.rs
```

### Two-layer IR (future)

A **language-neutral ShIR** sits between the shell AST and the
language-specific IRs.  It would model:

- `Assign { var, value }` (no sigils)
- `Output { value, newline }` (same across languages)
- `Command { name, args, redirects }` (shell commands)
- `Pipeline { stages }` (same concept in all shells)
- `Substitution { command }` (backtick, `$()` — language-agnostic)
- `TestExpr { op, lhs, rhs }` (file tests, string comparison)

The **Perl IR** (this document) then lowers from ShIR, adding Perl-isms
like sigils, `defined`, `qx{}`, special variables, etc.

### One-layer IR (now)

For now, the Perl IR directly consumes the shell AST via `RawText`
bridges.  The language-neutral layer can be added later without changing
the backend — it just changes what feeds the Perl IR.

### Key principle

Language-specific decisions (sigils, function naming conventions,
error handling patterns) are made at the **lowest IR layer**, not in
the generator.  This keeps the per-language backends small and the
shared analysis free of language bias.

---

## Current design: Perl-specific IR with RawText fallback

The IR models Perl semantics directly.  A `RawText` variant holds code
that hasn't been migrated yet, so conversion can happen function by
function.

```rust
// ── Expressions ──────────────────────────────────────────────────────

enum IrExpr {
    /// Integer literal
    Int(i64),
    /// String literal (content, interpolation style)
    Str(String, StrStyle),
    /// Variable: $name, @name, %name
    Var(String, Sigil),
    /// Array/hash element: $arr[idx], $map{key}
    Index { var: String, key: Box<IrExpr> },
    /// Binary operation
    BinOp { lhs: Box<IrExpr>, op: BinOpKind, rhs: Box<IrExpr> },
    /// Function call: foo(args...)
    Call { func: String, args: Vec<IrExpr> },
    /// Method call: $obj->method(args...)
    MethodCall { obj: Box<IrExpr>, method: String, args: Vec<IrExpr> },
    /// Ternary: cond ? then : else
    Ternary { cond: Box<IrExpr>, then: Box<IrExpr>, else_: Box<IrExpr> },
    /// Defined-or: expr // default
    DefinedOr { expr: Box<IrExpr>, default: Box<IrExpr> },
    /// String interpolation: "hello $name"
    Interpolate(Vec<InterpPart>),
    /// Raw Perl expression text (migration bridge)
    RawExpr(String),
}

enum Sigil { Scalar, Array, Hash }
enum BinOpKind { Add, Sub, Mul, Div, Mod, Pow, Concat, Eq, Ne, Lt, Gt, Le, Ge,
                 And, Or, Not, BitAnd, BitOr, BitXor, ShiftL, ShiftR }
enum StrStyle { SingleQuoted, DoubleQuoted, Command }
enum InterpPart {
    Lit(String),
    Expr(IrExpr),
}

// ── Statements ───────────────────────────────────────────────────────

enum IrStmt {
    /// Output: print/say with optional trailing newline
    Output { value: IrExpr, newline: bool },
    /// Assignment: $var = expr  or  ($var, $var2) = (expr, expr2)
    Assign { targets: Vec<AssignTarget>, expr: IrExpr },
    /// Local variable declaration: my $var  or  my $var = expr
    Declare { vars: Vec<Decl>, init: Option<IrExpr> },
    /// Array/hash assignment: @arr = (...);  %hash = (...);
    DeclareArray { var: String, sigil: Sigil, elements: Vec<IrExpr> },
    /// if/elsif/else
    If { cond: IrExpr, then: Vec<IrStmt>, elsifs: Vec<(IrExpr, Vec<IrStmt>)>,
         else_: Vec<IrStmt> },
    /// for my $var (@list) { ... }
    For { var: String, iter: IrExpr, body: Vec<IrStmt> },
    /// while (cond) { ... }
    While { cond: IrExpr, body: Vec<IrStmt> },
    /// do { ... } while/until
    DoWhile { body: Vec<IrStmt>, cond: IrExpr, until: bool },
    /// System call: system('cmd', @args)  or  $output = qx{cmd}
    System { cmd: IrExpr, args: Vec<IrExpr>, capture: Option<String> },
    /// Pipeline: multiple stages connected by pipes
    Pipeline { stages: Vec<Vec<IrStmt>>, last_output: Option<String> },
    /// Return from subroutine
    Return(Option<IrExpr>),
    /// Raw Perl text (migration bridge — functions not yet converted)
    RawText(String),
}

struct AssignTarget { var: String, sigil: Sigil, indices: Vec<IrExpr> }
struct Decl { name: String, sigil: Sigil }

// ── Top-level ────────────────────────────────────────────────────────

struct IrProgram {
    /// use strict; use warnings; etc. — auto-derived from constructs used
    imports: Vec<String>,
    /// Top-level statements
    stmts: Vec<IrStmt>,
    /// Subroutine definitions
    subs: Vec<IrSub>,
}

struct IrSub {
    name: String,
    params: Vec<String>,
    body: Vec<IrStmt>,
}
```

## Backend: `ir_to_perl()`

A single function converts `IrProgram` → Perl text. Style decisions live
here, not in the generator:

```rust
fn ir_to_perl(prog: &IrProgram) -> String {
    let mut out = String::new();
    emit_header(&mut out, &prog.imports);
    for stmt in &prog.stmts {
        emit_stmt(&mut out, stmt, 0);
    }
    for sub in &prog.subs {
        emit_sub(&mut out, sub);
    }
    out
}
```

### Style rules (example — adjustable)

| Pattern in IR | Current output (ugly) | Future output (clean) |
|---|---|---|
| `Output { value: Var("x"), newline: true }` | `print $x;\nif (!($x =~ m{\n\z}msx)) { print "\n"; }` | `say $x;` |
| `Output { value: Expr, newline: false }` | same wrapping | `print EXPR;` |
| `Declare { vars: [Scalar("x")], init: None }` | `my $x;` | `my $x;` (no change) |
| `If { cond: Var("x"), then: [...] }` | `if (($x)) {` | `if ($x) {` |
| `System { capture: Some("out") }` | `my $out = do { ... qx{...} ... };` | `my $out = qx{...};` |
| Pipeline with 1 stage | wraps in vars, pipes, etc. | just the stage body |

### RawText handling

```rust
fn emit_stmt(out: &mut String, stmt: &IrStmt, indent: usize) {
    match stmt {
        IrStmt::RawText(text) => {
            // Splice verbatim — no transformation, no style fixes
            out.push_str(text);
        }
        IrStmt::Output { value, newline } => {
            // Style choice lives here:
            if *newline {
                emit_indent(out, indent);
                out.push_str(&format!("say {};\n", ir_expr_to_perl(value)));
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("print {};\n", ir_expr_to_perl(value)));
            }
        }
        // ...
    }
}
```

## Migration strategy

```
┌──────────┐    ┌──────────────┐    ┌────────────┐
│ Shell AST │───>│  Generator   │───>│   String   │  (current)
└──────────┘    └──────────────┘    └────────────┘

┌──────────┐    ┌──────────────┐    ┌────────────┐    ┌────────────┐
│ Shell AST │───>│  Generator   │───>│  Perl IR   │───>│  ir_to_    │───> String
└──────────┘    └──────────────┘    │ (with some  │    │  perl()    │
                                    │  RawText )  │    └────────────┘
                                    └────────────┘
```

Steps:

1. **Define** the IR types and `ir_to_perl()` in a new module `src/ir.rs`.
2. **Add `RawText(String)`** to both `IrExpr` and `IrStmt`.
3. **Wrap the generator**: the top-level `generate()` currently returns
   `String`. Change it to produce `IrProgram` where every statement is
   `IrStmt::RawText(original_text)`. Call `ir_to_perl()` to get the
   final string. Tests pass (identical output).
4. **Pick one function** — say `generate_echo_command()` — and rewrite it
   to return `IrStmt::Output { ... }` instead of `RawText(...)`. The
   `IrExpr` for the value may still be `RawExpr(...)` at first.
5. **Work outward**: each migrated function produces cleaner IR, and the
   backend gradually sees more semantic nodes. Fix style issues in the
   backend, not in the generator.
6. **Eventually** no `RawText` remains. The backend can now be freely
   improved for style without touching generator logic.

## Benefits

- **Style fixes go in one place** (`ir_to_perl()`), not in 80+ generator functions
- **Multi-language backends**: replace `ir_to_perl()` with `ir_to_python()`
- **Optimization passes** as MIR transforms between generator and backend:
  - Dead assignment elimination
  - Constant folding
  - Import minimization
  - Variable lifetime shortening
- **Testable IR** — you can write unit tests that check `IrProgram` structures
  without running full end-to-end tests
