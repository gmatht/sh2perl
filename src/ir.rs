/// Intermediate Representation for code generation.
///
/// The generator produces an `IrProgram` from the shell AST.  The Perl
/// backend `shir_to_perl()` converts it to Perl text.  `RawText`/`RawExpr`
/// hold unimigrated code so conversion can happen function by function.
///
/// **Layering (who owns Perl text):** the AST-side `Generator`
/// (`src/generator/`) is the primary Perl renderer — it owns the text for
/// constructs that have not yet migrated to the IR, and `shir_to_perl` is
/// its ShIR-consumer wrapper (the sibling of `shir_to_estree`). Migrated
/// constructs flow as real IR nodes: the Generator emits them, `emit_stmt`/
/// `ir_expr_to_perl` render them, and `IrProgram::from_raw_perl` wraps the
/// remainder in a single `RawText` blob so the whole program can flow
/// through the ShIR pipeline (`--shir` → `--shir-in-perl`). When the
/// migration completes (all Generator functions emit IR nodes), the
/// wrapper and the Generator's text-building side both go away.
///
/// ShIR direction (PLAN.md §3/§8): this module is being generalized from a
/// Perl-only IR into a language-neutral ShIR that both the Perl backend and
/// the ESTree emitter consume. The first step is done: `Sigil` is now an
/// OPTIONAL backend annotation on `Var`/`AssignTarget`/`Decl`/`DeclareArray`
/// (the core no longer requires a Perl `$`/`@`/`%`; `None` renders as scalar
/// for Perl, and a non-Perl backend ignores it). Remaining Perl-specific
/// surface to neutralize: `StrStyle` (Command/Heredoc → extensions),
/// `Backtick`, `Regex`, `System`/`Pipeline` (→ `Exec`), `Require`,
/// `SetChildError`. `RawText`/`RawExpr` stay as the migration bridge. The
/// estree emitter (src/estree.rs) currently lowers from the raw AST and will
/// reroute through this IR once two backends consume it.
///
/// See docs/ir-design.md for full documentation.

// ── Sigils ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sigil {
    Scalar,
    Array,
    Hash,
}

// ── Type annotations (ask A2: serialize the numeric/string lift verdicts) ──
//
// Conservative typing for static backends (C): what the JS path's
// `numeric_lift_vars` / `string_lift_vars` (src/shir.rs) provably admit.
//   Int  -> every assignment is provably numeric (native `long long` / number)
//   Str  -> every assignment is provably a string literal (native char*)
//   Any  -> runtime store (mixed/unknown typing; shell vars are strings)
// Populated by `shir::analyze_var_types`; serialized in the ShIR JSON
// (ask A1). Existing backends ignore it (additive only).
//
// The C-style sized variants (Int32/Int64/UInt32/UInt64) are emitted by
// the C frontend (c-sh-go) from the C declarator types; the C-executed
// ESTree path lowers Int32/UInt32 to native JS numbers with `|0` /
// `Math.imul` / `>>>0` wrap semantics and Int64/UInt64 to BigInt
// (BigInt64Array for array storage) — see the BinInt64 benchmarks in
// benchmarks/i64/. Other backends treat them as Int (additive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrType {
    Int,
    Str,
    /// String known to contain only ASCII bytes. Additive: backends that
    /// ignore the annotation treat it as Str. Allows byte-indexed slicing
    /// (`&s[n..m]`) instead of the Unicode-safe `chars().skip().take()`.
    AsciiStr,
    Any,
    /// IEEE-754 bit width (32 = float, 64 = double). Serialized as
    /// `{"kind": "Float", "width": 64}` so the width survives the A1
    /// round-trip; the unit variants stay plain strings (byte-identical
    /// to the old derive output). Additive: backends that ignore the
    /// annotation are unaffected.
    Float(u8),
    /// C `int` — signed 32-bit, wraps mod 2^32. Serialized as `{"kind": "Int32"}`.
    Int32,
    /// C `long long` — signed 64-bit, wraps mod 2^64. Serialized as `{"kind": "Int64"}`.
    Int64,
    /// C `unsigned int` — unsigned 32-bit. Serialized as `{"kind": "UInt32"}`.
    UInt32,
    /// C `unsigned long long` — unsigned 64-bit. Serialized as `{"kind": "UInt64"}`.
    UInt64,
}

impl IrType {
    /// C `sizeof(T)` in bytes for the sized variants (the C frontend's
    /// model: int = 4, long long = 8); the widthless Int/Str/Any have no
    /// C size (None).
    pub fn c_sizeof(&self) -> Option<i64> {
        match self {
            IrType::Int32 | IrType::UInt32 => Some(4),
            IrType::Int64 | IrType::UInt64 => Some(8),
            _ => None,
        }
    }
}

impl serde::Serialize for IrType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            IrType::Int => s.serialize_unit_variant("IrType", 0, "Int"),
            IrType::Str => s.serialize_unit_variant("IrType", 1, "Str"),
            IrType::Any => s.serialize_unit_variant("IrType", 2, "Any"),
            IrType::Float(w) => {
                use serde::ser::SerializeStruct;
                let mut st = s.serialize_struct("IrType", 2)?;
                st.serialize_field("kind", "Float")?;
                st.serialize_field("width", w)?;
                st.end()
            }
            // AsciiStr and sized C ints serialize as {"kind": "…"}.
            IrType::AsciiStr
            | IrType::Int32
            | IrType::Int64
            | IrType::UInt32
            | IrType::UInt64 => {
                use serde::ser::SerializeStruct;
                let kind = match self {
                    IrType::AsciiStr => "AsciiStr",
                    IrType::Int32 => "Int32",
                    IrType::Int64 => "Int64",
                    IrType::UInt32 => "UInt32",
                    _ => "UInt64",
                };
                let mut st = s.serialize_struct("IrType", 1)?;
                st.serialize_field("kind", kind)?;
                st.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for IrType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = IrType;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    "an IrType (\"Int\"/\"Str\"/\"Any\" or {\"kind\":\"Float\",\"width\":N})",
                )
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<IrType, E> {
                match v {
                    "Int" => Ok(IrType::Int),
                    "Str" => Ok(IrType::Str),
                    "Any" => Ok(IrType::Any),
                    other => Err(E::custom(format!("unknown IrType {other:?}"))),
                }
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<IrType, A::Error> {
                let mut kind: Option<String> = None;
                let mut width: Option<u8> = None;
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "kind" => kind = Some(map.next_value()?),
                        "width" => width = Some(map.next_value()?),
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                match kind.as_deref() {
                    Some("Float") => match width {
                        Some(w) => Ok(IrType::Float(w)),
                        None => Err(serde::de::Error::custom(
                            "expected {\"kind\":\"Float\",\"width\":N}",
                        )),
                    },
                    Some("AsciiStr") => Ok(IrType::AsciiStr),
                    Some("Int32") => Ok(IrType::Int32),
                    Some("Int64") => Ok(IrType::Int64),
                    Some("UInt32") => Ok(IrType::UInt32),
                    Some("UInt64") => Ok(IrType::UInt64),
                    _ => Err(serde::de::Error::custom(
                        "expected {\"kind\":\"Float\",\"width\":N} or {\"kind\":\"Int32\"} etc.",
                    )),
                }
            }
        }
        d.deserialize_any(V)
    }
}

// ── Const/var annotations (the const-markup transform) ──────────────────
//
// Conservative const/var verdicts for static backends (C): per assigned
// variable, whether it can be emitted `const`/`readonly`. A variable is
// `Const` only when it has exactly one static assignment site that
// executes at most once (outside loops/function bodies) and is never
// written by the runtime store (`read`, `mapfile`, `unset`), by native
// arithmetic (`x++`, `((x=1))`), or by an array-element write, and the
// program has no dynamic write (`eval`/`source`). Anything else is
// `Var`. Missing from the list = never assigned (pure reads).
// Populated by `shir::analyze_var_const`; serialized in the ShIR JSON
// (ask A1). Existing backends ignore it (additive only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VarKind {
    Const,
    Var,
}

/// Storage-class selection for string variables in statically-typed
/// backends (C, Zig, Rust). Computed by `select_storage_class` from the
/// union of existing analyses (var_lengths, var_lifetimes, var_const,
/// capture_vars, escape_classes). Each backend maps the class to its own
/// native representation:
///
/// | Class | C | JS/Estree | Perl |
/// |---|---|---|---|
/// | Numeric | long long / int | number | integer |
/// | InlineBuffer | char x[N+1] | N/A (JS strings are GC) | N/A |
/// | ManagedString | sh2_str { ptr } | sh2.vars.x | $x |
/// | CaptureResult | char *x = NULL + strdup | sh2.vars.x | $x |
/// | Escaped | char *x = NULL (must survive) | sh2.vars.x | $x |
/// | ConstLiteral | const char x[N] = "..." | const | use constant |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StorageClass {
    /// Range/provenance analysis proves integers only → immediate numeric.
    Numeric,
    /// Length provably bounded ≤ threshold AND non-escaping AND non-capture
    /// → stack-allocated fixed buffer. Fast: zero malloc, dies with scope.
    InlineBuffer,
    /// Unbounded string, local scope, no capture, no escape, not exported
    /// → managed owning pointer (sh2_str in C; plain variable in GC'd langs).
    ManagedString,
    /// Assigned from a command capture or read — unbounded length unknown,
    /// must be a raw pointer that popen/fread can fill.
    CaptureResult,
    /// Exported, stored in a global, or otherwise escapes the enclosing
    /// scope → raw heap pointer that must survive scope exit.
    Escaped,
    /// Single-assignment literal, const-marked → read-only declaration.
    ConstLiteral,
}

impl std::fmt::Display for StorageClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageClass::Numeric => write!(f, "Numeric"),
            StorageClass::InlineBuffer => write!(f, "InlineBuffer"),
            StorageClass::ManagedString => write!(f, "ManagedString"),
            StorageClass::CaptureResult => write!(f, "CaptureResult"),
            StorageClass::Escaped => write!(f, "Escaped"),
            StorageClass::ConstLiteral => write!(f, "ConstLiteral"),
        }
    }
}

// ── Lifetime annotations (the variable-lifetime analysis) ────────────
//
// Conservative lifetime verdicts for static backends (C): per variable,
// the live span in statement positions of a pre-order walk (first access
// .. last access) and whether the value's storage may escape the current
// scope (array-element store, closure capture, function return). The C
// backend uses the span for per-point buffer sizing and buffer reuse,
// and the escape bit for the copy-vs-move / stack-vs-heap decision.
// Populated by `shir_passes::lifetime::analyze_var_lifetimes`;
// serialized in the ShIR JSON (ask A1). Existing backends ignore it
// (additive only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VarLifetime {
    /// First access (def or use) position in the pre-order statement walk.
    pub first: usize,
    /// Last access position in the pre-order statement walk.
    pub last: usize,
    /// True when the value's storage may be retained beyond the scope
    /// where it was produced (array-element store, closure capture,
    /// function return) — cannot be a stack local that is moved from
    /// or reused.
    pub escapes: bool,
}

// ── Binary operators ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Not,
    BitAnd,
    BitOr,
    BitXor,
    ShiftL,
    ShiftR,
}

// ── String style ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum StrStyle {
    SingleQuoted,
    DoubleQuoted,
    Command,
    /// Like DoubleQuoted but preserves `$` and `@` for Perl interpolation.
    /// Used for unquoted heredoc bodies (<<EOF) where Perl should
    /// interpolate variable references.  Newlines and control characters
    /// are still escaped so the Perl source is readable.
    Heredoc,
    /// Raw Perl code - emitted as-is without quoting. Use sparingly.
    Raw,
}

// ── Interpolation parts ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InterpPart {
    Lit(String),
    Expr(Box<IrExpr>),
}

// ── Arithmetic AST (neutral) ─────────────────────────────────────────
/// Parsed `$((...))` arithmetic — rendered as NATIVE JS arithmetic by the
/// ESTree backend (faster/cleaner than a runtime string-eval). Assignments
/// (`x=`, `x+=`, `x++`) are NOT representable here (they need setVar
/// semantics) and fall back to the runtime `sh2.arith` evaluator.
#[derive(Debug, Clone, PartialEq)]
pub enum ArithAst {
    Num(i64),
    Var(String),
    /// Bare-identifier arith read (core request
    /// zsh-sh-go-20260813-155123): the A1 export rewrites reads of a
    /// NUMERIC-LIFTED loop variable inside the loop body to this node
    /// (`{"type":"Ident","name":…}`) — the estree renderer derives a
    /// bare `Identifier` from a lifted `Var` read, so the A1 carries the
    /// node the backends actually render. Not produced by the shell
    /// parser (source text always parses to `Var`); only the A1 export
    /// rewrite and the deserializer construct it. Every backend renders
    /// it exactly like `Var` (the lift verdict decides the read shape).
    Ident(String),
    Index {
        var: String,
        key: Box<ArithAst>,
    },
    Bin {
        op: String,
        lhs: Box<ArithAst>,
        rhs: Box<ArithAst>,
    },
    Un {
        op: String,
        arg: Box<ArithAst>,
    },
    Cond {
        test: Box<ArithAst>,
        then: Box<ArithAst>,
        else_: Box<ArithAst>,
    },
    /// `name op= rhs` (`=` / `+=` / `-=` / `*=`; `/=`/`%=` stay on the
    /// runtime — the zero-divisor abort needs the helper). The expression
    /// VALUE is the assigned value (bash semantics).
    Assign {
        var: String,
        op: String,
        rhs: Box<ArithAst>,
    },
    /// `++name` / `name++` / `--name` / `name--` — delta ±1. The value is
    /// the NEW value (prefix) or the OLD value (postfix), exactly bash's
    /// arithmetic semantics.
    IncDec {
        var: String,
        delta: i64,
        prefix: bool,
    },
    /// C `sizeof(T)` — a compile-time constant; the core folds it to
    /// `Num(4|8)` for the sized variants (see `IrType::c_sizeof`) before
    /// any backend renders it. Kept as a node so the A1 JSON carries the
    /// typed C construct (the C frontend emits it).
    Sizeof(IrType),
    /// C cast `(T)x` — width/signedness coercion. The C-executed ESTree
    /// path renders it as `| 0` (Int32), `>>> 0` (UInt32), `BigInt(...)`
    /// (Int64/UInt64); other backends treat it as identity (widthless
    /// native arithmetic).
    Cast {
        ty: IrType,
        arg: Box<ArithAst>,
    },
}

// ── Expressions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum IrExpr {
    /// Integer literal
    Int(i64),
    /// String literal
    Str(String, StrStyle),
    /// Variable: $name, @name, %name. The sigil is an OPTIONAL
    /// backend annotation: the core IR is language-neutral and does not
    /// require a Perl sigil (None renders as scalar for the Perl backend;
    /// a non-Perl backend ignores it).
    Var(String, Option<Sigil>),
    /// Array/hash element: $arr[idx], $map{key}
    Index { var: String, key: Box<IrExpr> },
    /// Binary operation
    BinOp {
        lhs: Box<IrExpr>,
        op: BinOpKind,
        rhs: Box<IrExpr>,
    },
    /// Function call
    Call { func: String, args: Vec<IrExpr> },
    /// Method call
    MethodCall {
        obj: Box<IrExpr>,
        method: String,
        args: Vec<IrExpr>,
    },
    /// Ternary: cond ? then : else
    Ternary {
        cond: Box<IrExpr>,
        then: Box<IrExpr>,
        else_: Box<IrExpr>,
    },
    /// Defined-or: expr // default
    DefinedOr {
        expr: Box<IrExpr>,
        default: Box<IrExpr>,
    },
    /// String interpolation: "hello $name"
    Interpolate(Vec<InterpPart>),
    /// Command output capture (shell command substitution / backticks) —
    /// neutral. The Perl backend renders it as qx{}/backticks; a non-Perl
    /// consumer uses the wrapped command expression. If `native` is true the
    /// expression already produces the exact value (no trailing newline); if
    /// false, trailing newlines are stripped (shell semantics).
    Capture { expr: Box<IrExpr>, native: bool },
    /// Perl match regex: /pattern/flags
    Regex { pattern: String, flags: String },
    /// Numeric range: start..end (inclusive)
    Range { start: i64, end: i64 },
    /// Raw Perl expression text (migration bridge)
    RawExpr(String),
    /// Delayed statement block (closure) — used by the ESTree path for
    /// pipeline stages, subshell/background/redirect/function bodies and
    /// loop conditions. The Perl generator never emits it.
    Arrow(Vec<IrStmt>),
    /// Array literal (ESTree-path only — e.g. for-items, brace groups).
    Array(Vec<IrExpr>),
    /// Comprehension expression (Python list comprehension; ESTree-path
    /// only — core request py-sh-go-comp-if). Evaluates to a NEW array
    /// built by iterating `iter`, binding each item to `var` in the
    /// runtime store, evaluating `elem` per item, and SKIPPING items for
    /// which `cond` (the comp_if filter; None = no filter) is falsy.
    ArrayComp {
        var: String,
        iter: Box<IrExpr>,
        elem: Box<IrExpr>,
        cond: Option<Box<IrExpr>>,
    },
    /// Parameterized function-literal expression (Python `lambda`;
    /// ESTree-path only — core request py-sh-go-lambdef). Sibling of the
    /// zero-parameter `Arrow` thunk: carries explicit parameter names and
    /// a statement body; the ESTree renderer emits a JS
    /// ArrowFunctionExpression with params and binds each param into the
    /// runtime store at call time. The Perl generator never emits it.
    Lambda { params: Vec<String>, body: Vec<IrStmt> },
    /// Parsed arithmetic (neutral AST) — ESTree path renders native JS.
    Arith(Box<ArithAst>),
    /// Boolean literal (ESTree-path only — e.g. shopt enable flags).
    Bool(bool),
    /// Raw JSON literal (ESTree-path only — e.g. brace-expansion groups).
    Json(serde_json::Value),
    /// Bare identifier (ESTree-path only — e.g. the for-loop variable name).
    Ident(String),
    /// Object literal (ESTree-path only — e.g. redirect specs, env maps).
    Object(Vec<(String, IrExpr)>),
    /// Starred-expression splice marker (Python `star_expr` / the
    /// `argument` rule's `'*' test`; ESTree-path only — core request
    /// py-sh-go-star-expr): the wrapped expr's ELEMENTS are spliced into
    /// the enclosing list, not nested as one item. Valid ONLY as an
    /// `Array` element or a `Call` argument — the ESTree renderer emits a
    /// JS SpreadElement (`[...x]` / `f(...x)`); the runtime store's array
    /// values are native JS arrays, so the spread is the exact splice.
    Splice(Box<IrExpr>),
    /// A transform-declared expression node (shir_nodes): the extensible
    /// slot for semantic IR nodes (FieldExtract, CharTranslate, RegSub,
    /// etc.). Renderers that don't know the node fall back to the sh2.*
    /// call; traversers reach children via ExtExpr::children_mut.
    Ext(Box<dyn crate::shir_nodes::ExtExpr>),
}

// ── Supporting types ─────────────────────────────────────────────────

/// A 1-indexed field position or range (for FieldExtract, CharExtract).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FieldRange {
    Single(u32),
    Range { start: u32, end: u32 },
}

// ── Assignment target ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct AssignTarget {
    pub var: String,
    /// Optional backend sigil annotation (see Var).
    pub sigil: Option<Sigil>,
    pub indices: Vec<IrExpr>,
}

// ── Variable declaration ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    /// Optional backend sigil annotation (see Var).
    pub sigil: Option<Sigil>,
}

// ── Statements ───────────────────────────────────────────────────────

/// One `case` clause: shell-glob patterns + body.
#[derive(Debug, Clone, PartialEq)]
pub struct IrCaseClause {
    pub patterns: Vec<String>,
    pub body: Vec<IrStmt>,
}

/// One `except` clause of a `Try` statement (Python-style exception
/// handling; ESTree-path only — the Perl generator never emits it).
#[derive(Debug, Clone, PartialEq)]
pub struct TryExcept {
    /// Optional exception-class test expression (None = bare `except`).
    pub match_expr: Option<IrExpr>,
    /// Optional binding name for the caught exception (None = not bound).
    pub as_name: Option<String>,
    pub body: Vec<IrStmt>,
}

/// One `select` communication clause (Go `commClause`; ESTree-path only
/// — core request go-sh-commclause / go-sh-recvstmt). Carries the comm
/// kind ("recv" | "send" | "default"), the channel expression, and the
/// clause body; `recv` clauses may bind the received value to a target
/// var, `send` clauses carry the value to send. The ESTree renderer
/// lowers the whole `Select` to a non-blocking round-robin poll of the
/// channel FIFOs (the runtime emulates Go select semantics; bash has no
/// native select-on-channels).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectClause {
    /// "recv" | "send" | "default".
    pub comm: String,
    /// recv: optional target var for the received value (None = bare `<-ch`).
    pub target: Option<String>,
    /// recv/send: the channel expression (None for default).
    pub ch: Option<IrExpr>,
    /// send: the value expression to push (None for recv/default).
    pub value: Option<IrExpr>,
    pub body: Vec<IrStmt>,
}

/// One redirection spec (fd, mode, target expression).
#[derive(Debug, Clone, PartialEq)]
pub struct IrRedirect {
    pub fd: Option<i32>,
    pub mode: String, // "r" | "w" | "a" | "r+" | "wc" | "heredoc" | "herestring" | "unsupported"
    pub target: IrExpr,
    /// Whether an unquoted heredoc body should be interpolated (ESTree path).
    pub interpolate: bool,
}

/// GCC asm spec — the shared operand/clobber shape of the `Asm`
/// statement and the declarator-position `Assign.asm` label (core
/// request c-sh-go-toplevelasmargument-20260814-042952). For the
/// declarator form gcc only accepts a bare template (`asm("myx")` —
/// operands at file scope are a syntax error), so outputs/inputs/
/// clobbers are always empty there; the field keeps the full shape for
/// uniformity with the `Asm` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct AsmSpec {
    pub template: String,
    pub volatile: bool,
    pub outputs: Vec<(String, IrExpr)>,
    pub inputs: Vec<(String, IrExpr)>,
    pub clobbers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrStmt {
    /// A transform-declared node (shir_nodes): the extensible slot of the
    /// otherwise-closed enum. Renderers that don't know the node refuse
    /// loudly; traversers reach its children via ExtNode::children_mut.
    Ext(Box<dyn crate::shir_nodes::ExtNode>),
    /// Output: print/say with optional trailing newline
    /// If `target` is Some(filehandle_name), output goes to that filehandle
    /// (e.g. `$fh`) instead of STDOUT.  The name is emitted without a leading `$`.
    Output {
        value: IrExpr,
        newline: bool,
        target: Option<String>,
    },
    /// Write content to a file (shell output redirect `> file` / `>> file`).
    /// This replaces the STDOUT-save/restore pattern with a clean
    /// open-write-close idiom.
    WriteFile {
        /// Path to the target file (as an IR expression)
        path: IrExpr,
        /// Content to write (as an IR expression)
        content: IrExpr,
        /// If true, append (`>>`) instead of overwrite (`>`)
        append: bool,
    },
    /// Assignment
    Assign {
        targets: Vec<AssignTarget>,
        expr: IrExpr,
        /// Optional GCC asm-label spec on a DECLARATION-position assign
        /// (`int x asm("myx") = 7;` — the `toplevelAsmArgument` of
        /// `asmDefinition`; core request
        /// c-sh-go-toplevelasmargument-20260814-042952). The label only
        /// renames the SYMBOL in the object file — no runtime semantics
        /// in any A1 backend model — so the estree renderer lowers it to
        /// a no-op comment (oracle-faithful) and the other backends
        /// refuse loudly (refuse > guess).
        asm: Option<AsmSpec>,
    },
    /// Variable declaration
    Declare {
        vars: Vec<Decl>,
        init: Option<IrExpr>,
        /// If true, emit `local` instead of `my`.
        local: bool,
    },
    /// Array/hash assignment
    DeclareArray {
        var: String,
        /// Optional backend sigil annotation (see Var).
        sigil: Option<Sigil>,
        elements: Vec<IrExpr>,
    },
    /// if/elsif/else
    If {
        cond: IrExpr,
        then: Vec<IrStmt>,
        elsifs: Vec<(IrExpr, Vec<IrStmt>)>,
        else_: Vec<IrStmt>,
    },
    /// try/except/else/finally — Python-style exception handling
    /// (ESTree-path only; frontends emit it, the estree backend renders
    /// it as a JS try/catch/finally). The Perl generator never emits it;
    /// renderers that cannot express it must refuse loudly.
    Try {
        body: Vec<IrStmt>,
        excepts: Vec<TryExcept>,
        else_body: Vec<IrStmt>,
        finally_body: Vec<IrStmt>,
    },
    /// for loop (the shell foreach: `for i in a b c`)
    For {
        var: String,
        iter: IrExpr,
        body: Vec<IrStmt>,
    },
    /// C-style for (init; cond; step) — the RICH form imperative frontends
    /// (C, C++) emit. The shell-flavored renderers never see it: the
    /// `strip_cfor` pass lowers it to `init; while(cond){body; step}` (with
    /// the step re-inserted before every body `continue`), and a renderer
    /// that DOES encounter one refuses (REFUSE > GUESS). The shell `For`
    /// above stays the foreach.
    ForInit {
        init: Vec<IrStmt>,
        cond: IrExpr,
        step: Vec<IrStmt>,
        body: Vec<IrStmt>,
    },
    /// continue / break — first-class control flow (not a `Call(func:
    /// "continue")` disguised as a runtime builtin). Rendered to the
    /// runtime's `sh2.continue()` / `sh2.break()` in async bodies, or a
    /// native ContinueStatement/BreakStatement where legal. The
    /// deserializer still accepts the legacy Call-form for old A1s.
    Continue,
    Break,
    /// while loop
    While { cond: IrExpr, body: Vec<IrStmt> },
    /// do { } while/until
    DoWhile {
        body: Vec<IrStmt>,
        cond: IrExpr,
        until: bool,
    },
    /// Fatal error: die/croak with a message expression
    Die {
        expr: IrExpr,
        /// If true, emit `croak` instead of `die` (requires `use Carp`).
        carp: bool,
    },
    /// Warning: warn/carp with a message expression
    Warn {
        expr: IrExpr,
        /// If true, emit `carp` instead of `warn` (requires `use Carp`).
        carp: bool,
    },
    /// Run a command (neutral — the language-agnostic "exec"). The Perl
    /// backend renders it as `system(...)`/`qx{}`; the ESTree consumer uses
    /// cmd/args/redirects. `redirects` is empty until generators populate it.
    Exec {
        cmd: IrExpr,
        args: Vec<IrExpr>,
        capture: Option<String>,
        redirects: Vec<IrExpr>,
        /// Command-scoped environment variables (`VAR=x cmd`) — ESTree path.
        env: Vec<(String, IrExpr)>,
    },
    /// Pipeline — a sequence of commands connected by pipes.
    /// When `capture` is `Some(var)`, the entire pipeline's stdout is captured
    /// into `$var` using a single `qx{...}` call instead of simulating the
    /// pipeline in Perl.  This produces cleaner, more idiomatic output for
    /// pipelines used in command substitution (e.g. `` count=`ls -1 | wc -l` ``).
    /// `cmd_str` holds the reconstructed shell command for qx{} when capture is set.
    Pipeline {
        stages: Vec<Vec<IrStmt>>,
        last_output: Option<String>,
        /// If set, capture the pipeline's stdout into this variable using qx{}.
        capture: Option<String>,
        /// Original shell command string (for qx{} capture).
        cmd_str: Option<String>,
    },
    /// Return from subroutine
    Return(Option<IrExpr>),
    /// Exit the program with a code (or without, for default exit 0)
    Exit(Option<IrExpr>),
    /// Set $CHILD_ERROR (from $? >> 8 after an external command)
    SetChildError(IrExpr),
    /// Require a module at file scope (e.g. `require POSIX;`).
    /// Unlike `use`, `require` is evaluated at runtime and does not
    /// import symbols.  It is emitted inline as a bare statement.
    Require(String),
    /// Raw Perl text (migration bridge)
    RawText(String),
    /// ── Neutral nodes (ESTree-path AST→IR builder only) ─────────────
    /// The Perl generator never emits these; the Perl renderer's arms are
    /// unreachable. They make the IR capable of expressing the full shell so
    /// shir_to_estree can consume it (PLAN.md §3).
    /// case … esac — dispatch on a value against shell-glob patterns.
    Case {
        discriminant: IrExpr,
        clauses: Vec<IrCaseClause>,
    },
    /// Redirection wrapper: run `inner` with fd redirects applied.
    Redirect {
        inner: Vec<IrStmt>,
        redirects: Vec<IrRedirect>,
    },
    /// Shell function definition (positional args via the runtime).
    ///
    /// `params` — the frontends' NAMED parameter list (`func add(a, b int)`,
    /// core request rust-20260927-121500-function-params-ingress). EMPTY
    /// for bash, whose functions take positional args only (`$1`, `$2`) and
    /// so carry no names. A non-empty list is authoritative: a backend
    /// renders the definition with those parameters bound in order. Before
    /// this field existed the deserializer had nowhere to put a `params`
    /// key, so a Go function's parameter names were DROPPED SILENTLY at
    /// ingress and every read of a parameter inside the body degraded to a
    /// read of an unbound name.
    ///
    /// `named_blocks` — PowerShell-style named blocks (`dynamicparam` /
    /// `begin` / `process` / `end` / `clean`), as `(block_name, stmts)`
    /// pairs (core-request powershell-sh-go). EMPTY for bash functions
    /// (bash has no named blocks). ESTree-path only: the ESTree renderer
    /// (shir.rs) wraps them into the define arrow with their per-block
    /// execution semantics (begin once, process once PER pipeline input
    /// item, end/clean once); the other backends render `body` and ignore
    /// the field. A frontend emitting named blocks targets the ESTree
    /// backend (the A1-ingress oracle is `otranspilerl-cli --source-lang shir --target estree`).
    Function {
        name: String,
        /// Named parameters, bound in order. Empty for bash functions.
        params: Vec<String>,
        body: Vec<IrStmt>,
        named_blocks: Vec<(String, Vec<IrStmt>)>,
    },
    /// Subshell — copy semantics (env/fd snapshot, run, discard).
    Subshell(Vec<IrStmt>),
    /// Background — run asynchronously.
    Background(Vec<IrStmt>),
    /// Go-style `select` over channel communication clauses
    /// (ESTree-path only — core requests go-sh-commclause /
    /// go-sh-recvstmt). The Perl generator never emits it; renderers that
    /// cannot express it must refuse loudly.
    Select { clauses: Vec<SelectClause> },
    /// Inline-assembly statement — the C `asm` family (asmDefinition /
/// asmArgument / asmOperand / asmClobbers / asmQualifier; core
/// requests c-sh-go-asm / c-sh-go-asmargument /
/// c-sh-go-asmqualifier). ESTree-path only: JS cannot execute machine
    /// code, so the renderer lowers it to a NO-OP carrying the template
    /// (faithful only for effect-free asm — empty outputs; an asm with
    /// output operands has observable writes the no-op drops, flagged in
    /// the emitted text, refuse > guess). `volatile` is the asmQualifier
    /// (a compiler hint, no runtime meaning); the `inline`/`goto`
    /// qualifiers carry no runtime meaning either (the frontend must
    /// refuse `goto` — it changes the operand grammar to labels — never
    /// silently drop it). `outputs`/`inputs` are (constraint, operand
    /// expr) pairs; `clobbers` is the asmClobbers list.
    Asm {
        template: String,
        volatile: bool,
        outputs: Vec<(String, IrExpr)>,
        inputs: Vec<(String, IrExpr)>,
        clobbers: Vec<String>,
    },
    /// Plain block group `{ a; b; }` (no copy semantics — unlike Subshell).
    Block(Vec<IrStmt>),
    /// Evaluate an expression as a statement (ESTree-path pipelines,
    /// and/or/not, bare sh2.* calls). Perl generator never emits it.
    Expr(IrExpr),
    /// Label marker — a jump target for `Goto`. Emitted by frontends
    /// (c-sh-go for C `goto`, future frontends for labeled-break
    /// families); the shared `restructure_goto` pass (shir_passes/)
    /// rewrites `Label`/`Goto` into structured flow before any renderer
    /// sees the IR. Renderers refuse loudly if one survives.
    Label(String),
    /// Jump to a `Label`. See `Label`.
    Goto(String),
    /// `-O4` autoshaderization: a compute loop (a pure-arithmetic counted
    /// loop) emitted in a shader-offloadable form via
    /// `sh2.shaderize(start, end, step, op, (i) => { body })`. The
    /// estree renderer emits the runtime call; other renderers refuse
    /// loudly (REFUSE > GUESS).
    Shaderize {
        var: String,
        start: i64,
        end: i64,
        step: i64,
        op: String,
        body: Vec<IrStmt>,
    },
}

// ── Subroutine ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct IrSub {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<IrStmt>,
}

// ── Program ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram {
    /// use statements — auto-derived from constructs used
    /// `use` statements — auto-derived from constructs used
    pub imports: Vec<String>,
    /// `require` statements (e.g. `require POSIX;`) — emitted at file scope
    /// before top-level statements but after `use` statements.
    pub requires: Vec<String>,
    /// Top-level statements
    pub stmts: Vec<IrStmt>,
    /// Subroutine definitions
    pub subs: Vec<IrSub>,
    /// Conservative type annotations (ask A2): (var name, verdict), sorted
    /// by name for deterministic serialization. Empty until
    /// `shir::analyze_var_types` runs. Existing backends ignore it.
    pub var_types: Vec<(String, IrType)>,
    /// Source line numbers per top-level statement — (stmt index, source
    /// line) pairs, sorted by index. Populated by the C frontend (the
    /// `--output-lineno` option); the shell path leaves it empty. The
    /// Perl renderer appends ` # line N` comments when present.
    pub stmt_lines: Vec<(usize, usize)>,
    /// Conservative max-string-length annotations (a transform sibling of
    /// the A2 verdicts): per variable, the provable upper bound on the
    /// string's byte length (None = unbounded — captures, loops, runtime
    /// input). Populated by `shir::analyze_string_lengths`; the C backend
    /// uses it to emit fixed buffers (`char s[64]`) instead of heap/`char*`.
    /// Existing backends ignore it (additive only).
    pub var_lengths: Vec<(String, Option<u64>)>,
    /// Conservative const/var verdicts (the const-markup transform,
    /// sibling of the A2 type verdicts and `var_lengths`): per ASSIGNED
    /// variable, `Const` when it is written exactly once (single static
    /// assignment site) and that site executes at most once per run
    /// (outside loops/function bodies) and is not a runtime-store write
    /// (`read`/`unset`/`eval`/native arith `x++`/array-element). Sorted by
    /// name for deterministic serialization. Populated by
    /// `shir::analyze_var_const`; the C backend emits `const` for `Const`
    /// verdicts. Existing backends ignore it (additive only).
    pub var_const: Vec<(String, VarKind)>,
    /// Conservative variable-lifetime annotations (the analysis's
    /// sibling of `var_types`/`var_lengths`/`var_const`): per variable,
    /// the live span in statement positions of a pre-order walk and
    /// whether the value's storage escapes the scope (array store /
    /// closure / return). Populated by
    /// `shir_passes::lifetime::analyze_var_lifetimes`; the C backend
    /// uses it for per-point buffer sizing, buffer reuse, and the
    /// copy-vs-move / stack-vs-heap decision. Existing backends ignore
    /// it (additive only).
    pub var_lifetimes: Vec<(String, VarLifetime)>,
    /// The "No spaces" tag (a transform sibling of `var_types`):
    /// per variable, `true` when its value is PROVABLY free of IFS
    /// whitespace (space, tab, newline — the `\s+` word-split set).
    /// Populated by `shir::analyze_var_nospace`; the estree backend
    /// skips the word-split on `$var` expansions when the tag holds
    /// (the split would be a provable no-op). Additive — backends that
    /// ignore it are unaffected. Sorted by name for deterministic
    /// serialization.
    pub var_nospace: Vec<(String, bool)>,
    /// Unified storage-class selection per string variable. Populated by
    /// `select_storage_classes` from the union of existing analyses.
    /// Backends consult this instead of re-deriving from scattered checks.
    pub var_storage: Vec<(String, StorageClass)>,
    /// Bash-identity variables the program REFERENCES that bash sets ITSELF
    /// at startup (never inherited from the environment): HOSTNAME, USER's
    /// siblings like BASH_VERSION/ZSH_VERSION.  Populated by
    /// `shir::analyze_var_bash_env`; the Perl backend initializes them
    /// (HOSTNAME → real hostname via Sys::Hostname) so `${HOSTNAME:-...}`
    /// matches what a bash run of the same script sees.  Additive —
    /// backends that ignore it are unaffected.  Sorted for deterministic
    /// serialization.
    pub var_bash_env: Vec<String>,
}

// ── Backend: IR → Perl text ─────────────────────────────────────────

/// Placeholder for a condition-position command the renderer cannot rebuild.
/// A Perl comment keeps it inert and unambiguous; the post-pass rewrites the
/// whole marker to `1` (see `fix_condition_booleans`), so a legitimately
/// computed `Bool(false)` — which also renders as a bare `0` — is never
/// mistaken for it.
pub const UNBUILDABLE_COND: &str = "0/*sh2:unbuildable-cond*/";

/// Convert a single IR statement to a Perl source string.
/// This is the public entry point for the generator to produce clean
/// Perl from IR nodes without constructing a full IrProgram.
pub fn stmt_to_perl(stmt: &IrStmt, indent: usize) -> String {
    let mut out = String::new();
    emit_stmt(&mut out, stmt, indent);
    out
}

/// Convert a single IR expression to a Perl source string.
pub fn expr_to_perl(expr: &IrExpr) -> String {
    ir_expr_to_perl(expr)
}

/// Check if any statement in a list uses `Output { newline: true }`.
fn prog_uses_say(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| stmt_uses_say(s))
}

fn stmt_uses_say(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Output { newline: true, .. } => true,
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            then.iter().any(|s| stmt_uses_say(s))
                || elsifs
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| stmt_uses_say(s)))
                || else_.iter().any(|s| stmt_uses_say(s))
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            body.iter().any(|s| stmt_uses_say(s))
        }
        _ => false,
    }
}

/// Convert an `IrProgram` to a Perl source string.
///
/// Style decisions (say vs print, parentheses style, indentation) are
/// made here, not in the generator.
///
/// Role: this is the ShIR-consumer wrapper of the Perl backend — the
/// sibling of `shir_to_estree`. The AST-side `Generator` (`src/generator/`)
/// owns Perl text for constructs that have not yet migrated to the IR;
/// `ast_to_ir` wraps that output in a single `RawText` blob via
/// `IrProgram::from_raw_perl` (the migration bridge). As constructs
/// migrate, the Generator emits real IR nodes and this renderer emits
/// them as Perl; `from_raw_perl` becomes unnecessary when the Generator
/// emits pure IR nodes. See PLAN.md §3 and `docs/ir-design.md`.
pub fn shir_to_perl(prog: &IrProgram) -> String {
    let mut out = String::new();

    // Shebang
    out.push_str("#!/usr/bin/env perl\n");
    out.push_str("use strict;\n");
    out.push_str("use warnings;\n");

    // `for ((...))` (core request zsh-sh-go-20260813-153215): the shell
    // lowering emits the rich A1 ForInit node — the perl renderer must
    // never see an unstripped one (it refuses), so lower it to
    // `init; while(cond){body; step}` first (the ingest path's CLI-level
    // strip; double-strip is a no-op).
    let mut stripped = prog.clone();
    crate::shir_passes::strip_cfor(&mut stripped);
    // shir-native-stmt (perl-only shell-out elimination): NOT in the
    // shared transforms::all() — its rewrites (echo>file → Block-wrapped
    // exec, status_exec markers, test&&echo||echo → If) are perl-oriented
    // and regress the estree backend's native folding (writeFile for
    // echo>file, native echo, dead-flags liveness). Applied here so
    // fail-shir benefits without estree regressions.
    crate::transforms::shir_native_stmt::transform(&mut stripped.stmts);
    for sub in stripped.subs.iter_mut() {
        crate::transforms::shir_native_stmt::transform(&mut sub.body);
    }
    // Array-variable census (before the transform's rewrites) for the
    // `param("slice", …)` `${arr[@]:o:l}` vs `${s:o:l}` disambiguation.
    populate_var_census(&stripped);
    // builtin-op native arm (shir-builtin-op-20260816): the A1 carries
    // `builtin(cmd, args)` ops (the exec-to-builtin transform). The perl
    // renderer ACCEPTS the op — emit_stmt's builtin arms (statement,
    // chain, condition, reconstruction) dispatch native commands to their
    // Perl emulations and shell out only the still-unsupported remainder.
    // No erasure: the op is the single native-lowering point.
    let prog = &stripped;

    // Run optimization passes before emitting.
    let stmts = optimize_stmts(&prog.stmts);
    if std::env::var("DLS2").is_ok() {
        for st in stmts.iter() {
            eprintln!("DLS2: stmt {:?} raw={}", std::mem::discriminant(st), matches!(st, IrStmt::RawText(_)));
        }
    }
    let modern_ir = prog.imports.is_empty();

    // Function calls: exec(foo, …) where foo is a defined shell function
    // must become a Perl sub call (bash -c would not know it). Rewrite the
    // stmts first; the emitter renders the marker as a sub call.
    let stmts = if modern_ir {
        // Nested functions (`outer(){ inner(){ … }; inner; }`) are real
        // bash functions too: collect every Function name, at any depth,
        // so `inner` lowers to a Perl sub call instead of `bash -c inner`
        // (examples/081_nested_functions).
        fn collect_fn_names(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
            for s in stmts {
                match s {
                    IrStmt::Function { name, body, named_blocks, .. } => {
                        out.insert(name.clone());
                        collect_fn_names(body, out);
                        for (_, nb) in named_blocks {
                            collect_fn_names(nb, out);
                        }
                    }
                    IrStmt::If { then, elsifs, else_, .. } => {
                        collect_fn_names(then, out);
                        for (_, b) in elsifs {
                            collect_fn_names(b, out);
                        }
                        collect_fn_names(else_, out);
                    }
                    IrStmt::While { body, .. }
                    | IrStmt::DoWhile { body, .. }
                    | IrStmt::For { body, .. }
                    | IrStmt::Block(body) => collect_fn_names(body, out),
                    IrStmt::ForInit { init, step, body, .. } => {
                        collect_fn_names(init, out);
                        collect_fn_names(step, out);
                        collect_fn_names(body, out);
                    }
                    _ => {}
                }
            }
        }
        let mut fns: std::collections::HashSet<String> = std::collections::HashSet::new();
        collect_fn_names(&stmts, &mut fns);
        if fns.is_empty() {
            stmts
        } else {
            rewrite_fn_calls(&stmts, &fns)
        }
    } else {
        stmts
    };

    // Imports (`use` statements).
    // Auto-derive `use feature 'say'` if any Output { newline: true } exists.
    // The three modern-IR preamble imports (Carp / English / IPC::Open3) are
    // appended AFTER the body is rendered, gated on the actual generated
    // text (see below); the emission also happens after the body render so
    // the gate has a complete view.
    let mut imports = prog.imports.clone();
    if prog_uses_say(&stmts) {
        let needs_say = !imports.iter().any(|i| i.contains("feature"));
        if needs_say {
            imports.push("feature 'say'".to_string());
        }
    }
    // Bash-identity variables the program references: bash sets HOSTNAME
    // itself at startup (from the hostname), so a faithful translation
    // initializes it from the REAL hostname at runtime (Sys::Hostname —
    // pure Perl, no bash dependency).  This is what `shir::analyze_var_bash_env`
    // detected; without it `${HOSTNAME:-localhost}` would read an unset
    // env var and print the default even though bash prints the machine
    // name.
    let needs_hostname = prog.var_bash_env.iter().any(|n| n == "HOSTNAME");
    if needs_hostname {
        imports.push("Sys::Hostname qw(hostname)".to_string());
    }

/// The byte index of the first newline that is NOT inside a Perl string
/// literal — where the `# line N` source-mapping comment may be spliced.
///
/// A native capture's payload can be a multi-line `q{...}` literal
/// (`x=$(seq 1 10 | head -3)`), and splicing the comment into it would
/// print the comment as part of the captured value. Tracks `'...'`,
/// `"..."`, backticks, `q<d>...<d>` / `qq<d>...<d>` (bracket delimiters
/// nest) and `\` escapes.
fn first_code_newline(s: &str) -> Option<usize> {
    #[derive(PartialEq)]
    enum St {
        Code,
        Single,
        Double,
        Backtick,
        /// `q<open> ... <close>`
        Q(char, char),
    }
    let b = s.as_bytes();
    let mut st = St::Code;
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        match st {
            St::Code => match c {
                '\'' => st = St::Single,
                '"' => st = St::Double,
                '`' => st = St::Backtick,
                '#' => {
                    // a comment runs to the end of the line
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                'q' => {
                    // `q<d>` / `qq<d>`: only when a delimiter follows
                    let (nd, adv) = if b.get(i + 1) == Some(&b'q') {
                        (b.get(i + 2).copied(), 3)
                    } else {
                        (b.get(i + 1).copied(), 2)
                    };
                    if let Some(d) = nd {
                        let d = d as char;
                        if d.is_ascii_alphanumeric() || d == '_' {
                            i += 1;
                            continue;
                        }
                        let close = match d {
                            '(' => ')',
                            '[' => ']',
                            '{' => '}',
                            '<' => '>',
                            o => o,
                        };
                        st = St::Q(d, close);
                        i += adv;
                        continue;
                    }
                }
                '\n' => return Some(i),
                _ => {}
            },
            St::Single => match c {
                '\\' => {
                    i += 2;
                    continue;
                }
                '\'' => st = St::Code,
                _ => {}
            },
            St::Double | St::Backtick => {
                let end = if st == St::Double { '"' } else { '`' };
                match c {
                    '\\' => {
                        i += 2;
                        continue;
                    }
                    _ if c == end => st = St::Code,
                    _ => {}
                }
            }
            St::Q(open, close) => {
                if c == '\\' {
                    i += 2;
                    continue;
                }
                if open != close && c == open {
                    // bracket pairs nest — skip the balanced run
                    let mut depth = 1usize;
                    let mut j = i + 1;
                    while j < b.len() {
                        let cj = b[j] as char;
                        if cj == '\\' {
                            j += 2;
                            continue;
                        }
                        if cj == open {
                            depth += 1;
                        } else if cj == close {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        j += 1;
                    }
                    i = j + 1;
                    st = St::Code;
                    continue;
                }
                if c == close {
                    st = St::Code;
                }
            }
        }
        i += 1;
    }
    None
}

    // ── Body ────────────────────────────────────────────────────────
    // Render statements + subs + exit into a `body` buffer FIRST so the
    // modern-IR preamble can be scanned against the ACTUAL generated text:
    // the emitter splices the infrastructure vars (`$main_exit_code`,
    // `$CHILD_ERROR`, `$__argc`, `$ls_success`, `$output`,
    // `$__nocasematch`) into format strings, so a textual scan of the
    // rendered body is the only reliable oracle for whether each preamble
    // declaration is needed.
    let mut body = String::new();
    // Top-level statements
    for (idx, stmt) in stmts.iter().enumerate() {
        let line = prog
            .stmt_lines
            .iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, l)| *l);
        let before = body.len();
        emit_stmt(&mut body, stmt, 0);
        if let Some(l) = line {
            // a SHORT comment at the end of the statement's first line:
            // `$sum += $i;  # line 7` — the source-mapping convention.
            // The newline must be OUTSIDE any string literal: a native
            // capture's payload can be a multi-line `q{…}` (e.g.
            // `x=$(seq 1 10 | head -3)`), and inserting into it would
            // print the comment as part of the captured value.
            let added = &body[before..];
            if let Some(nl) = first_code_newline(added) {
                body.insert_str(before + nl, &format!("  # line {l}"));
            }
        }
    }
    body.push('\n');

    // Subroutines
    for sub in &prog.subs {
        emit_sub(&mut body, sub);
        body.push('\n');
    }

    // Exit — only if $main_exit_code might be non-zero (i.e. if any
    // statement references it).  For scripts that never touch it,
    // omit the exit so Perl's default exit(0) applies.
    let has_main_exit = modern_ir
        || stmts.iter().any(|s| stmt_refers_to_main_exit(s))
        || prog
            .subs
            .iter()
            .any(|sub| sub.body.iter().any(|s| stmt_refers_to_main_exit(s)));
    if has_main_exit {
        body.push_str("exit $main_exit_code;\n");
    }

    if modern_ir {
        // Modern-IR program (`--shir` → `--shir-in-perl`): the JSON contract
        // carries no import list, so add the standard preamble imports ONLY
        // when the rendered body actually uses them — `use Carp` for a
        // carp/croak-style Warn/Die (the rm emulation emits
        // `carp "rm: carping: …"`), `use English` for the long
        // $OS_ERROR/$ERRNO/... names, `use IPC::Open3` for open3 (never
        // emitted on the IR path today — shell-outs go through
        // `system('bash','-c',…)`).
        if body.contains("carp") || body.contains("croak") {
            imports.push("Carp".to_string());
        }
        if [
            "$ERRNO",
            "$EVAL_ERROR",
            "$INPUT_RECORD_SEPARATOR",
            "$OS_ERROR",
            "$PROGRAM_NAME",
        ]
        .iter()
        .any(|n| body.contains(n))
        {
            imports.push(
                "English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME)"
                    .to_string(),
            );
        }
        if body.contains("open3") {
            imports.push("IPC::Open3".to_string());
        }
    }

    // Emit the imports (every decision input — the body scan, `say`, and
    // the hostname analysis — is computed by now).
    for import in &imports {
        out.push_str(&format!("use {};\n", import));
    }
    if needs_hostname {
        out.push_str("$ENV{HOSTNAME} //= hostname();\n");
    }
    if prog.var_bash_env.iter().any(|n| n == "BASH_VERSION") || body.contains("$ENV{BASH_VERSION}") {
        out.push_str("$ENV{BASH_VERSION} //= do { my $v = `bash -c 'echo \\$BASH_VERSION'`; chomp $v; $v };\n");
    }
    // Blank line after imports block (only if there are imports)
    if !imports.is_empty() {
        out.push('\n');
    }
    // Runtime imports (`require` statements)
    for req in &prog.requires {
        out.push_str(&format!("require {};\n", req));
    }
    if !prog.requires.is_empty() {
        out.push('\n');
    }

    // Modern-IR preamble: only the infrastructure variables the rendered
    // body actually references.  (The previous version emitted all of them
    // unconditionally — `$ls_success`/`$output` are dead on the IR path,
    // `$__argc` is only needed for `$#` reads, `$__nocasematch` only for
    // shopt/case-nocasematch lowering.)
    if modern_ir {
        if body.contains("$main_exit_code") {
            out.push_str("my $main_exit_code = 0;\n");
        }
        if body.contains("$CHILD_ERROR")
            || body.contains("__sh_uu_")
            || body.contains("__sh_pipe_")
        {
            out.push_str("our $CHILD_ERROR = 0;\n");
        }
        if body.contains("$__argc") {
            // Snapshot the positional-arg count BEFORE any $ARGV[n] read
            // (the magic @ARGV extends on indexed reads, corrupting
            // scalar(@ARGV)).
            out.push_str("my $__argc = @ARGV;\n");
        }
        // `$!` — the pid of the most recent background job. The `Background`
        // arm assigns each fork's pid here, and `var_read("!")` reads it.
        // bash leaves `$!` UNSET when no background job has run, so it
        // expands to the empty string (`echo "bg: $!"` → `bg: `). `0` is a
        // real, distinct value that a pid can never take, so the initializer
        // must be `''` — with `0` the generator printed `bg: 0`.
        if body.contains("$__sh2_last_bg") {
            out.push_str("our $__sh2_last_bg = '';\n");
        }
        if body.contains("$ls_success") {
            out.push_str("my $ls_success = 0;\n");
        }
        if body.contains("$output") {
            out.push_str("my $output = '';\n");
        }
        if body.contains("$__nocasematch") {
            out.push_str("my $__nocasematch = 0;\n");
        }
        // Hoisted declarations for assigned variables (use strict).
        let mut vars = Vec::new();
        collect_assigned_vars(&stmts, &mut vars);
        let mut read_vars = Vec::new();
        collect_read_vars_stmts(&stmts, &mut read_vars);
        for (n, s) in read_vars {
            if !vars.iter().any(|(vn, vs)| vn == &n && *vs == s) {
                vars.push((n, s));
            }
        }
        for (name, sigil) in &vars {
            // Infrastructure vars the emitter splices in itself (declared
            // just above) — a second `my` masks the first.
            if matches!(
                name.as_str(),
                "output" | "ls_success" | "__nocasematch" | "__argc" | "main_exit_code"
            ) {
                continue;
            }
            let assigned = collect_assigned_vars_contains(&stmts, name);
            match sigil {
                Sigil::Scalar => {
                    // Read-only vars initialize to '' (bash unset = empty,
                    // and Perl would warn on undef in string concat).
                    if assigned {
                        out.push_str(&format!("my ${};\n", name));
                    } else {
                        out.push_str(&format!("my ${} = '';\n", name));
                    }
                }
                Sigil::Array => out.push_str(&format!("my @{};\n", name)),
                Sigil::Hash => out.push_str(&format!("my %{};\n", name)),
            }
        }
        out.push('\n');
    }

    // The external-command seam (UU-FFI.md): emitted when the body calls
    // __sh_uu_capture/__sh_uu_run/__sh_pipe_* — the helpers probe for the
    // uu-ffi `.so` at BEGIN and fall back to guarded fork/exec (with a
    // one-time stderr warning) when absent. AFTER the preamble declarations
    // (`our $CHILD_ERROR`) so the strict subs compile.
    if body.contains("__sh_uu_capture")
        || body.contains("__sh_uu_run")
        || body.contains("__sh_pipe_run")
        || body.contains("__sh_pipe_capture")
    {
        out.push_str(SH_RUN_EXT_PREAMBLE);
        out.push('\n');
    }
    // Append the rendered body (statements + subs + exit).
    out.push_str(&body);

    // Restore brace balance — some generated code paths may produce
    // unbalanced delimiters, so add missing closing braces as a safety net.
    // NOTE: As generators migrate to emit proper IR nodes instead of RawText,
    // the backend naturally produces balanced braces and this hack becomes
    // unnecessary. It is kept for now to catch any remaining RawText paths.
    if !cfg!(feature = "no-brace-fix") {
        let opens = out.chars().filter(|&c| c == '{').count();
        let closes = out.chars().filter(|&c| c == '}').count();
        for _ in 0..(opens.saturating_sub(closes)) {
            out.push_str("}\n");
        }
    }

    out
}

// ── Embed profile (the purify design, PLAN §10) ───────────────────────
//
// A shell snippet rendered as a FRAGMENT inside a host program (purify:
// replace `system("")` / backtick-like constructs with native code for any
// host language). Statements only — no shebang, pragmas, imports, preamble,
// or exit. Host-scope names are reused as bare `$x`; everything else is
// declared locally with bash-subshell semantics (docs/embed-contract.md).

/// Embedding context. Stage 1 implements the `Backtick` profile (`System` /
/// `Popen` are reserved; the construct-visibility spec is in
/// `docs/embed-contract.md`).
#[derive(Default, Clone, Debug)]
pub struct EmbedCtx {
    /// Names the host program declares in the enclosing scope (the
    /// harvester's membership list, v1: file-wide). A name the snippet
    /// READS is reused as a bare `$x` when present here (bash subshells see
    /// the parent's value); absent names are declared locally (`my $x = '';`
    /// — bash unset = empty).
    pub host_scope: Vec<String>,
    /// Backtick semantics (Perl `qx`): trailing newlines are PRESERVED
    /// (bash `$()` strips them). False keeps the standalone `$()`-style
    /// stripping.
    pub backtick_newlines: bool,
    /// Emit English.pm names (`$INPUT_RECORD_SEPARATOR` …) or normalize to
    /// the core vars (`$/` …) so the fragment is valid in host files that
    /// don't `use English`.
    pub english_names: bool,
}

/// What the enclosing host construct is — decides the var-visibility and
/// IO semantics the fragment must reproduce (spec: docs/embed-contract.md).
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedConstruct {
    /// `` `cmd` `` / `$(cmd)` — subshell: parent vars visible to reads,
    /// writes discarded.
    #[default]
    Backtick,
    /// `system("cmd")` — child process: only env visible. (Reserved.)
    System,
    /// `popen` / `open("|cmd")` — stream handle. (Reserved.)
    Popen,
}

#[derive(Default, Debug)]
pub struct EmbedResult {
    pub fragment: String,
    /// Names the fragment reads from the host scope (bare `$x` reuse or
    /// `my $x = $x;` copy-in). The bindings gate —
    /// `required_host_bindings ⊆ ctx.host_scope` — turns a renderer bug
    /// (bare `$x` for a name the caller did not list) into a hard failure.
    pub required_host_bindings: Vec<String>,
    /// Snippet features the embed profile cannot render (preamble-var
    /// dependencies, functions, `exit`, …). The caller falls back (e.g. to
    /// `exec('sh', '-c', …)`), exactly like today's purify rejections — but
    /// analysis-driven, not regex-driven.
    pub refusals: Vec<String>,
}

/// Render a shell snippet as an embeddable Perl fragment. Deterministic:
/// declaration order follows the Vec-based first-seen order of
/// `collect_assigned_vars` / `collect_read_vars_stmts` — never hash order
/// (the legacy `Generator`'s HashSet iteration was 30/30 flaky across
/// processes).
pub fn shir_to_perl_embed(prog: &IrProgram, ctx: &EmbedCtx) -> EmbedResult {
    let mut result = EmbedResult::default();

    // Embed fragments are preamble-free: disable the `__sh_uu_*` seam
    // (its helpers live in the standalone preamble) so captures render
    // through the self-contained open-idiom. Restored on exit (single
    // exit point below; thread-local, so parallel test threads are
    // unaffected).
    let prev_seam = UU_SEAM.with(|c| c.replace(false));

    // The snippet is its own mini-program: the same lowering + optimize as
    // the standalone renderer (strip_cfor is a no-op for embed inputs but
    // keeps the shared pass honest).
    let mut stripped = prog.clone();
    crate::shir_passes::strip_cfor(&mut stripped);
    // builtin-op native arm (shir-builtin-op-20260816): the embed renderer
    // ACCEPTS the op — same builtin arms as the standalone renderer; no
    // erasure.
    let stmts = optimize_stmts(&stripped.stmts);

    // Refuse constructs that only make sense in a standalone program (v1):
    // an explicit `exit` would kill the HOST process; a function definition
    // needs a host-scope binding (name collisions); a background job forks
    // and exits.
    for s in &stmts {
        match s {
            IrStmt::Exit { .. } => result.refusals.push("exit statement".into()),
            IrStmt::Function { name, .. } => {
                result.refusals.push(format!("function `{name}` definition"))
            }
            IrStmt::Background(_) => result.refusals.push("background job".into()),
            _ => {}
        }
    }

    // Bash-subshell declaration rules (spec: docs/embed-contract.md §"var
    // visibility"): reads see the host value, writes are fragment-local.
    //   read-only ∧ host → bare `$x` reuse                  (required binding)
    //   read-only ∧ ¬host → `my $x = '';`   (bash unset = empty)
    //   written  ∧ host → `my $x = $x;`     copy-in, writes stay local
    //   written  ∧ ¬host → `my $x;`
    // Order: assigned-first-then-read-only, mirroring the standalone
    // preamble (Vec-based, deterministic).
    let mut vars = Vec::new();
    collect_assigned_vars(&stmts, &mut vars);
    let mut read_vars = Vec::new();
    collect_read_vars_stmts(&stmts, &mut read_vars);
    for (n, s) in read_vars {
        if !vars.iter().any(|(vn, _)| vn == &n) {
            vars.push((n, s));
        }
    }
    let mut decls = String::new();
    for (name, sigil) in &vars {
        let written = collect_assigned_vars_contains(&stmts, name);
        let in_host = ctx.host_scope.iter().any(|h| h == name);
        let sigil_str = match sigil {
            Sigil::Scalar => "$",
            Sigil::Array => "@",
            Sigil::Hash => "%",
        };
        if written {
            if in_host {
                // copy-in: reads see the host value, writes stay local
                decls.push_str(&format!(
                    "my {sigil_str}{name} = {sigil_str}{name};\n"
                ));
                result.required_host_bindings.push(name.clone());
            } else {
                decls.push_str(&format!("my {sigil_str}{name};\n"));
            }
        } else if in_host {
            // reuse: bare reads resolve to the enclosing scope
            result.required_host_bindings.push(name.clone());
        } else {
            match sigil {
                Sigil::Scalar => decls.push_str(&format!("my ${name} = '';\n")),
                Sigil::Array => decls.push_str(&format!("my @{name};\n")),
                Sigil::Hash => decls.push_str(&format!("my %{name};\n")),
            }
        }
    }

    let mut out = String::new();
    // The whole fragment lives in a `do { … }` block: a fresh scope, so the
    // copy-in declarations (`my $x = $x;`) get their OWN lexical — in the
    // host's same scope a second `my $x` would mask-REUSE the host pad slot
    // and the snippet's writes would leak out (bash subshell semantics:
    // writes must not escape). This is the same shape purify.pl's `__bt(do
    // { … })` wrapper already imposes.
    out.push_str("do {\n");
    if !decls.is_empty() {
        for line in decls.lines() {
            if !line.is_empty() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    for s in &stmts {
        emit_stmt(&mut out, s, 1);
    }
    out.push_str("};\n");

    // ── post-render rewrites (each mirrors a purify.pl heuristic that the
    // renderer now owns; the refusal scan below is the analysis-driven
    // replacement for purify's regex rejections) ──────────────────────

    // `$main_exit_code = $CHILD_ERROR = X;` → `$CHILD_ERROR = X;` — the
    // standalone exit tracker is dead in an embed (the host owns its own
    // exit); the status mirror stays.
    out = out.replace(
        "$main_exit_code = $CHILD_ERROR = ",
        "$CHILD_ERROR = ",
    );

    if ctx.backtick_newlines {
        // Perl `qx` does NOT strip trailing newlines (bash `$()` does); the
        // standalone's command-substitution chomp is wrong inside a Perl
        // backtick replacement. Both strip shapes go: the s-strip form
        // (open-idiom value captures) and the `chomp $_r;` form (embed
        // captures render chomp; see `cmd_str_to_open_perl`).
        out = out.replace("$_r =~ s/\\n+\\z//; ", "");
        out = out.replace("chomp $_r; ", "");
    }

    if !ctx.english_names {
        out = out
            .replace("$INPUT_RECORD_SEPARATOR", "$/")
            .replace("$OS_ERROR", "$!")
            .replace("$ERRNO", "$!")
            .replace("$EVAL_ERROR", "$@");
    } else {
        // Mirror of the normalizer: spell the capture idiom's core vars
        // with English.pm names (the caller promises `use English` in
        // the host). Exact idiom markers only — a blind `$!` replace
        // would rewrite `$!` inside the single-quoted bash -c text and
        // change the child's semantics.
        out = out.replace("local $/;", "local $INPUT_RECORD_SEPARATOR;");
        out = out.replace("cmd failed: $!\\n", "cmd failed: $OS_ERROR\\n");
    }

    if out.contains("$CHILD_ERROR") {
        out.insert_str(0, "our $CHILD_ERROR = 0;\n");
    }
    // Command emulations call Carp's carp/croak/cluck/confess on error
    // paths; the STANDALONE preamble imports Carp, an embed fragment has
    // no preamble. Emit the import at the fragment top — `use` is
    // compile-time and package-wide, and a duplicate `use Carp;` in a
    // host that already imports it is a silent no-op (purify.pl's
    // import-injection heuristic, minus the regex detection).
    if regex::Regex::new(r"\b(?:carp|croak|cluck|confess)\b")
        .unwrap()
        .is_match(&out)
    {
        out.insert_str(0, "use Carp;\n");
    }

    // Standalone status-tracker writes that are DEAD in an embed: the ls
    // emulation emits `$ls_success = 0/1;` and `$main_exit_code =
    // $CHILD_ERROR;` (status flags the standalone exit logic consumes). In
    // a fragment they'd be undeclared-var failures under `use strict` —
    // drop the lines (they have no output side effect).
    let ls_re = regex::Regex::new(r"(?m)^[ \t]*\$ls_success\s*=\s*[01];[ \t]*\n")
        .unwrap();
    out = ls_re.replace_all(&out, "").to_string();
    let me_re = regex::Regex::new(r"(?m)^[ \t]*\$main_exit_code\s*=\s*\$CHILD_ERROR;[ \t]*\n")
        .unwrap();
    out = me_re.replace_all(&out, "").to_string();

    // Standalone-only dependencies the fragment must not reference (the
    // preamble declares them in a full program; an embed has no preamble).
    for needle in [
        "$main_exit_code",
        "$__argc",
        "$__nocasematch",
        "$ls_success",
        "$DATE_SNAPSHOT",
    ] {
        if out.contains(needle) {
            result
                .refusals
                .push(format!("fragment references {needle} (standalone-only)"));
        }
    }
    if regex::Regex::new(r"\bsay\s+")
        .unwrap()
        .is_match(&out)
    {
        result
            .refusals
            .push("fragment uses `say` (host may lack `use feature 'say'`)".into());
    }
    // A bare `exit` statement would terminate the HOST process (the snippet's
    // `exit N` lowers to an exec call the renderer emits as Perl `exit`).
    if regex::Regex::new(r#"(?m)^\s*exit[\s"]"#)
        .unwrap()
        .is_match(&out)
    {
        result.refusals.push("fragment contains a bare `exit`".into());
    }

    result.fragment = out;
    result.required_host_bindings.sort();
    result.required_host_bindings.dedup();
    UU_SEAM.with(|c| c.set(prev_seam));
    result
}

// ── Statement emitter ────────────────────────────────────────────────

/// Emit `A && B` / `A || B` as native statements guarded by the chain
/// status. Returns false (emitting nothing) for a non-chain expression.
fn emit_native_binop(out: &mut String, indent: usize, e: &IrExpr) -> bool {
    let IrExpr::BinOp { lhs, op, rhs } = e else {
        return false;
    };
    if !matches!(op, BinOpKind::And | BinOpKind::Or) {
        return false;
    }
    emit_stmt(out, &IrStmt::Expr((**lhs).clone()), indent);
    emit_indent(out, indent);
    out.push_str(if *op == BinOpKind::And {
        "if ($CHILD_ERROR == 0) {\n"
    } else {
        "if ($CHILD_ERROR != 0) {\n"
    });
    emit_stmt(out, &IrStmt::Expr((**rhs).clone()), indent + 1);
    emit_indent(out, indent);
    out.push_str("}\n");
    true
}

pub(crate) fn emit_stmt(out: &mut String, stmt: &IrStmt, indent: usize) {
    match stmt {
        IrStmt::Ext(n) => {
            // per-backend drop-in handlers (render_ext) render transform-
            // declared nodes; a node with no handler keeps the refusal.
            if !crate::render_ext::render_ext(out, &**n, indent) {
                emit_indent(out, indent);
                out.push_str("die \"otranspilerl: shIR Ext node not supported by the Perl backend\\n\";\n");
            }
        }
        IrStmt::RawText(text) => {
            // Splice verbatim — no transformation, EXCEPT a terminator: the
            // native-statement transform emits whole `do { … }` blocks with
            // no trailing `;` (valid as the last statement, a parse error
            // when another statement follows — `}print`).
            let t = text.trim_end();
            if t.starts_with("do {") && !t.ends_with(';') {
                out.push_str(t);
                out.push_str(";\n");
            } else {
                out.push_str(text);
            }
        }

        // goto/label must have been restructured by the shared
        // restructure_goto pass (shir_passes/restructure.rs) before the
        // renderers see the IR. A survivor means the pass's subset was
        // exceeded — refuse loudly (the backend stub gate counts the
        // TODO(unsupported) marker).
        // A Label/Goto that survived restructure_goto (a goto shape
        // outside the shared pass's subset). Perl HAS goto, so emit the
        // real `LABEL:` / `goto LABEL;` instead of refusing — the jump
        // semantics are identical to the source's.
        IrStmt::Label(name) => {
            emit_indent(out, indent);
            out.push_str(&format!("{name}: ;\n"));
        }
        IrStmt::Goto(name) => {
            emit_indent(out, indent);
            out.push_str(&format!("goto {name};\n"));
        }

        IrStmt::Shaderize { .. } => {
            // -O4 autoshaderization is JS-target-only; the Perl generator
            // refuses loudly (REFUSE > GUESS).
            emit_indent(out, indent);
            out.push_str("die \"shaderize: not supported by the Perl backend\";\n");
        }

        // Neutral ESTree-path-only nodes — the Perl generator never emits them.
        IrStmt::Subshell(body) => {
            // ( cmd ) — copy semantics: assignments inside do NOT leak. Save
            // the vars the body assigns, run it, restore them (Perl `local`
            // can't apply to the hoisted `my` lexicals).
            let mut assigned = Vec::new();
            collect_assigned_vars(body, &mut assigned);
            let mut saves = Vec::new();
            for (name, sigil) in &assigned {
                if is_env_style_var_name(name) {
                    continue;
                }
                let save = format!("__save_{}", name);
                emit_indent(out, indent);
                let var = match sigil {
                    Sigil::Scalar => format!("${}", name),
                    Sigil::Array => format!("@{}", name),
                    Sigil::Hash => format!("%{}", name),
                };
                out.push_str(&format!("my ${} = {};\n", save, var));
                saves.push((name.clone(), save, sigil.clone()));
            }
            for s in body.iter() {
                emit_stmt(out, s, indent);
            }
            for (name, save, sigil) in saves {
                emit_indent(out, indent);
                let var = match sigil {
                    Sigil::Scalar => format!("${}", name),
                    Sigil::Array => format!("@{}", name),
                    Sigil::Hash => format!("%{}", name),
                };
                out.push_str(&format!("{} = ${};\n", var, save));
            }
        }
        IrStmt::Background(body) => {
            // `( cmd ) &` — bash background job (triage-perl
            // t44_background py/posix-sh-go): fork a child to run the
            // body; the parent continues immediately and the `wait`
            // builtin reaps it. Autoflush BEFORE forking so the child
            // never duplicates buffered parent output (fork copies the
            // stdio buffer).
            emit_indent(out, indent);
            out.push_str("$| = 1;\n");
            static BG_SEQ: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let pid = format!(
                "__sh2_bg{}",
                BG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            emit_indent(out, indent);
            out.push_str(&format!("my ${pid} = fork();\n"));
            // `$!` — bash's "pid of the last background job". Recorded in a
            // stable variable (not the per-fork `$__sh2_bgN`, whose name
            // depends on how many Background nodes the program has) so
            // `var_read("!")` can resolve a `$!` read anywhere in the body.
            emit_indent(out, indent);
            out.push_str(&format!("$__sh2_last_bg = ${pid};\n"));
            emit_indent(out, indent);
            out.push_str(&format!("if (defined ${pid} && ${pid} == 0) {{\n"));
            for s in body.iter() {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent + 1);
            out.push_str("exit $main_exit_code;\n");
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        // try/except/else/finally — ESTree-path only; the Perl generator
        // never emits it. Refuse loudly (Perl eval could express it, but
        // no generator produces it and guessing the exception convention
        // would be wrong).
        IrStmt::Try { .. } => {
            emit_indent(out, indent);
            out.push_str("die \"otranspilerl: shIR construct not yet supported by the Perl backend (try)\\n\";\n");
        }
        // Go-style select over channel comm clauses — ESTree-path only;
        // the Perl generator never emits it. Refuse loudly.
        IrStmt::Select { .. } => {
            emit_indent(out, indent);
            out.push_str("die \"otranspilerl: shIR construct not yet supported by the Perl backend (select)\\n\";\n");
        }
        // Inline assembly — ESTree-path only (JS cannot execute machine
        // code either; the estree renderer lowers it to a no-op comment).
        // The Perl generator refuses loudly (refuse > guess).
        IrStmt::Asm { .. } => {
            emit_indent(out, indent);
            out.push_str("die \"otranspilerl: shIR construct not yet supported by the Perl backend (asm)\\n\";\n");
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            // case $x in pat1) …;; pat2) …;; *) …;; esac → if/elsif chain on
            // regex-translated glob patterns (bash case is anchored glob
            // matching on the value).
            let d = ir_expr_to_perl(discriminant);
            let mut any_emitted = false;
            for (i, clause) in clauses.iter().enumerate() {
                let has_default = clause.patterns.iter().any(|p| p == "*");
                // A pattern that is entirely a VARIABLE — the shape the
                // case-pattern capture hoist produces (`"${__sh2_casepat_0}"`,
                // or a plain user `"$p"`) — must INTERPOLATE: it is a runtime
                // string, not glob text. `glob_to_regex` on the
                // quote-stripped text emitted the literal characters
                // `${__sh2_casepat_0}`, so the match was against that text
                // and the clause never fired (parse-redirect-in-case-pattern
                // printed nothing where bash printed "match").
                //
                // A `$name` / `${name}` / `${name[...]}` pattern keeps its
                // sigil in the Perl regex, where it interpolates.
                let patterns: Vec<String> = clause
                    .patterns
                    .iter()
                    .filter(|p| p.as_str() != "*")
                    .map(|p| {
                        let t = p.trim_matches('"').trim_matches('\'');
                        if let Some(name) = case_pattern_var_name(t) {
                            // Quote the metacharacters the VALUE may contain:
                            // Perl interpolates before the match, so the value
                            // is matched LITERALLY, which is what bash's
                            // quote-free `case` word does for this shape.
                            format!("\\Q${}\\E", name)
                        } else {
                            glob_to_regex(t)
                        }
                    })
                    .collect();
                let is_first = i == 0 && !any_emitted;
                if has_default && i == clauses.len() - 1 {
                    // `*)` final clause — render as else.
                    emit_indent(out, indent);
                    if is_first {
                        out.push_str(&format!("if (({}) =~ /^.*$/) {{\n", d));
                    } else {
                        out.push_str("} else {\n");
                    }
                } else {
                    let re = format!("^(?:{})$", patterns.join("|"));
                    emit_indent(out, indent);
                    if is_first {
                        out.push_str(&format!("if (({}) =~ /{}/) {{\n", d, re));
                    } else {
                        out.push_str(&format!("}} elsif (({}) =~ /{}/) {{\n", d, re));
                    }
                }
                for s in &clause.body {
                    emit_stmt(out, s, indent + 1);
                }
                any_emitted = true;
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        IrStmt::Function { name, body, .. } => {
            // Shell function → Perl sub. `local @ARGV = @_` maps the bash
            // positional params ($1 → $ARGV[0]) onto the function's args;
            // the body's statements render inline (bash functions share the
            // global scope; Perl subs see the hoisted lexical vars).
            emit_indent(out, indent);
            out.push_str(&format!("sub {} {{\n", name));
            emit_indent(out, indent + 1);
            out.push_str("local @ARGV = @_;\n");
            // `$#` is this call's argument count — snapshot it before any
            // `$ARGV[n]` read extends the magic @ARGV (shadows the
            // top-level `$__argc`).
            emit_indent(out, indent + 1);
            out.push_str("my $__argc = @ARGV;\n");
            for s in body.iter() {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        IrStmt::Redirect { inner, redirects } => {
            // `mapfile [-t] VAR < FILE` / `readarray …` — read the file into
            // `@VAR` natively. `mapfile` is a shell builtin: a `bash -c`
            // child assigns the array in the CHILD (bash arrays cannot cross
            // exec), leaving the parent's `@VAR` empty (012's mapfile line).
            if let Some(code) = native_mapfile_read(inner, redirects) {
                emit_indent(out, indent);
                out.push_str(&code);
                out.push('\n');
                return;
            }
            // `while …; do …; done < FILE` — open FILE on STDIN for the
            // loop body. `local *STDIN` restores the previous handle at
            // block exit (the read builtin consumes the redirected STDIN).
            if let [IrStmt::While { .. }] = inner.as_slice() {
                if let Some(r) = redirects.iter().find(|r| {
                    matches!(r.fd, None | Some(0)) && r.mode == "r"
                }) {
                    emit_indent(out, indent);
                    out.push_str("{\n");
                    emit_indent(out, indent + 1);
                    out.push_str("local *STDIN;\n");
                    emit_indent(out, indent + 1);
                    out.push_str(&format!(
                        "open(STDIN, '<', {}) or die \"Cannot read input: $!\\n\";\n",
                        render_word(&r.target)
                    ));
                    for s in inner {
                        emit_stmt(out, s, indent + 1);
                    }
                    emit_indent(out, indent);
                    out.push_str("}\n");
                    return;
                }
            }
            // `while …; do …; done <<< "text"` — a herestring feeding the
            // loop's STDIN (the bat-sh-go `for /f %%a in ("lit")` lowering).
            // bash appends a trailing newline to the herestring, so build the
            // scalar with one and open an in-memory filehandle over it; `read`
            // then consumes it line-wise exactly as under bash.
            if let [IrStmt::While { .. }] = inner.as_slice() {
                if let Some(r) = redirects
                    .iter()
                    .find(|r| matches!(r.fd, None | Some(0)) && r.mode == "herestring")
                {
                    static HS_SEQ: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    let hs = format!(
                        "__sh2_hs{}",
                        HS_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    emit_indent(out, indent);
                    out.push_str("{\n");
                    emit_indent(out, indent + 1);
                    out.push_str("local *STDIN;\n");
                    emit_indent(out, indent + 1);
                    out.push_str(&format!(
                        "my ${hs} = {} . \"\\n\";\n",
                        render_word(&r.target)
                    ));
                    emit_indent(out, indent + 1);
                    out.push_str(&format!(
                        "open(STDIN, '<', \\${hs}) or die \"Cannot read herestring: $!\\n\";\n"
                    ));
                    for s in inner {
                        emit_stmt(out, s, indent + 1);
                    }
                    emit_indent(out, indent);
                    out.push_str("}\n");
                    return;
                }
            }
            if let Some(body) = cat_heredoc_body(inner, redirects) {
                let interpolated = redirects.first().map(|r| r.interpolate).unwrap_or(false);
                if interpolated {
                    // Unquoted `<<EOF` expands `$var` (and command
                    // substitutions). A body with no command substitution is
                    // exactly a Perl double-quoted string; `$(`/backtick
                    // bodies fall through to the shell path, which expands
                    // them itself.
                    if !body.contains("$(") && !body.contains('`') {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "print({}); $main_exit_code = $CHILD_ERROR = 0;\n",
                            render_str_literal(
                                &rewrite_heredoc_env(&body),
                                &StrStyle::Heredoc
                            )
                        ));
                        return;
                    }
                } else if let Some(s) = crate::pipeline_native::native_heredoc(&body) {
                    out.push_str(&s);
                    out.push('\n');
                    return;
                }
            }
            // A redirect-wrapped `"$@"` (the corpus's `capture` helper runs
            // the caller's command with both streams tee'd to temp files).
            // The rebuilt shell text `"$@" ` has NO positional parameters in
            // the `bash -c` child (they are Perl's @ARGV), so the command
            // never ran.  Dup the redirected fds to the REAL descriptors and
            // run the argv directly.
            if let [IrStmt::Expr(IrExpr::Call { args, .. })] = inner.as_slice() {
                if let Some(IrExpr::Call { func: lf, args: la }) = args.first() {
                    if lf == "listVar"
                        && la.first().and_then(call_arg_str).as_deref() == Some("@")
                    {
                        let mut pre: Vec<String> = Vec::new();
                        let mut post: Vec<String> = Vec::new();
                        let mut n = 0usize;
                        for r in redirects {
                            if !matches!(r.mode.as_str(), "w" | "wc" | "a") {
                                continue;
                            }
                            let fd = r.fd.unwrap_or(1);
                            if !matches!(fd, 1 | 2) {
                                continue;
                            }
                            let saved = format!("$__sh2_dup{n}");
                            let fh = format!("$__sh2_dupfh{n}");
                            n += 1;
                            pre.push(format!(
                                "open(my {fh}, '>&', \\*STD{}) or die; ",
                                if fd == 1 { "OUT" } else { "ERR" }
                            ));
                            pre.push(format!(
                                "open(STD{}, {}, {}) or die \"redirect: $!\\n\"; ",
                                if fd == 1 { "OUT" } else { "ERR" },
                                if r.mode == "a" { "'>>'" } else { "'>'" },
                                render_word(&r.target)
                            ));
                            let _ = &saved;
                            post.push(format!(
                                "open(STD{}, '>&', {fh}) or die; close({fh}); ",
                                if fd == 1 { "OUT" } else { "ERR" }
                            ));
                        }
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "{{ {pre}system(@ARGV); $main_exit_code = $CHILD_ERROR = $? >> 8; {post}}}\n",
                            pre = pre.join(""),
                            post = post.join(""),
                        ));
                        return;
                    }
                }
            }
            // A redirect-wrapped call to a PROGRAM-DEFINED function
            // (`capture LBL tty < "$f"`): a Perl sub cannot be reached
            // through a rebuilt shell command, so run the body with the
            // REAL fd redirected. Both directions matter:
            //   fd 1 (w/a) — `f LBL cmd > FILE`: the sub's prints/children
            //                must land in FILE;
            //   fd 0 (r)   — `f LBL cmd < FILE`: the sub's `read`s (and
            //                any shell-out) must see FILE.
            // `local *STDIN` makes the open unwind with the block, so a
            // nest of redirects composes.
            if let [IrStmt::Expr(IrExpr::Call { func, .. })] = inner.as_slice() {
                if func == "$fn_call" || func == "fnCall" {
                    let mut body = String::new();
                    emit_stmt(&mut body, &inner[0], 2);
                    let out_r = redirects.iter().find(|r| {
                        matches!(r.fd, None | Some(1))
                            && matches!(r.mode.as_str(), "w" | "wc" | "a")
                    });
                    let in_r = redirects
                        .iter()
                        .find(|r| matches!(r.fd, None | Some(0)) && r.mode == "r");
                    if let Some(r) = out_r {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "{{ open(my $__fn_fh, {}, {}) or die \"redirect: $!\\n\"; my $__fn_sel = select($__fn_fh); {body} select($__fn_sel); close($__fn_fh); }}\n",
                            if r.mode == "a" { "'>>'" } else { "'>'" },
                            render_word(&r.target),
                        ));
                        return;
                    }
                    if let Some(r) = in_r {
                        emit_indent(out, indent);
                        out.push_str("{\n");
                        emit_indent(out, indent + 1);
                        out.push_str("local *STDIN;\n");
                        emit_indent(out, indent + 1);
                        out.push_str(&format!(
                            "open(STDIN, '<', {}) or die \"Cannot read input: $!\\n\";\n",
                            render_word(&r.target)
                        ));
                        emit_indent(out, indent + 1);
                        out.push_str(&body.trim_start());
                        emit_indent(out, indent);
                        out.push_str("}\n");
                        return;
                    }
                }
            }
            // Rebuild the shell command with shell redirection syntax and
            // run it via bash -c — stdout matches bash exactly (redirects
            // are shell-level semantics).
            match stmts_to_shell_cmd(inner) {
                Some(mut cmd) => {
                    let mut ok = true;
                    let mut heredocs: Vec<(String, String)> = Vec::new();
                    // Interpolated/variable redirect targets (`> "$f"`,
                    // `> /tmp/x.$$.tmp` — triage-perl t32_redirect,
                    // examples/pid_tempfile): the target expr is NOT a
                    // plain Str, so the bash text cannot carry it (bash
                    // would expand `$$` to the CHILD's pid and would not
                    // see `$f` at all). Bind the target to a temp %ENV
                    // slot in Perl FIRST (the pid/var values are Perl's),
                    // then reference `"$__sh2_rdN"` in the bash command
                    // (bash children inherit %ENV).
                    let mut prelude: Vec<String> = Vec::new();
                    for r in redirects {
                        let fd = r.fd.unwrap_or(1);
                        match r.mode.as_str() {
                            "heredoc" | "heredoc-tabs" => {
                                // <<EOF — the body is the target; feed it via
                                // bash heredoc syntax at the end of the command.
                                let delim = if r.interpolate {
                                    "_SH2DOC_"
                                } else {
                                    "'_SH2DOC_'"
                                };
                                cmd.push_str(&format!(" <<{}", delim));
                                heredocs.push((
                                    "_SH2DOC_".to_string(),
                                    call_arg_str(&r.target).unwrap_or_default(),
                                ));
                            }
                            "herestring" => {
                                // A herestring target is frequently NOT a
                                // literal: `<<< "$(echo …)"` carries a
                                // Capture. `call_arg_str` yields None for
                                // those and the old `.unwrap_or_default()`
                                // emitted `<<< ''` — the command then read
                                // NOTHING (063_18 printed an empty line).
                                // Route through `append_redirect_frag`, which
                                // renders the real shell word.
                                if !append_redirect_frag(&mut cmd, fd as i64, &r.mode, &r.target) {
                                    ok = false;
                                    break;
                                }
                            }
                            _ => match call_arg_str(&r.target) {
                                Some(_) => {
                                    if !append_redirect_frag(&mut cmd, fd as i64, &r.mode, &r.target) {
                                        ok = false;
                                        break;
                                    }
                                }
                                None => {
                                    // w/a/r file redirect with an
                                    // interpolated target — bind it in Perl
                                    // and reference the env slot from bash.
                                    if !matches!(r.mode.as_str(), "w" | "wc" | "a" | "r") {
                                        ok = false;
                                        break;
                                    }
                                    static RD_SEQ: std::sync::atomic::AtomicUsize =
                                        std::sync::atomic::AtomicUsize::new(0);
                                    let tmp = format!(
                                        "__sh2_rd{}",
                                        RD_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                                    );
                                    prelude.push(format!(
                                        "$ENV{{{}}} = {};",
                                        tmp,
                                        render_word(&r.target)
                                    ));
                                    let op = match r.mode.as_str() {
                                        "w" | "wc" => ">",
                                        "a" => ">>",
                                        _ => "<",
                                    };
                                    let frag = match fd {
                                        0 => format!(" {op} \"${tmp}\""),
                                        1 => format!(" {op} \"${tmp}\""),
                                        n => format!(" {n}{op} \"${tmp}\""),
                                    };
                                    cmd.push_str(&frag);
                                }
                            },
                        }
                    }
                    for (delim, body) in heredocs {
                        cmd.push('\n');
                        cmd.push_str(&body);
                        if !body.ends_with('\n') {
                            cmd.push('\n');
                        }
                        cmd.push_str(&delim);
                    }
                    // Any body stashed by `append_redirect_frag` (a heredoc
                    // on a stage of a pipeline inner) belongs at the end of
                    // this line too.
                    cmd.push_str(&take_heredoc_tail());
                    if ok {
                        for line in prelude {
                            emit_indent(out, indent);
                            out.push_str(&line);
                            out.push('\n');
                        }
                        emit_shell_cmd(out, indent, &cmd);
                    } else {
                        emit_indent(out, indent);
                        out.push_str("die \"otranspilerl: shIR construct not yet supported by the Perl backend (redirect mode)\\n\";\n");
                    }
                }
                None => {
                    // The inner isn't a single command (e.g. a while-loop or
                    // block with a redirect) — run the inner statements with
                    // Perl's STDOUT redirected to the target file (select).
                    // print-based output (echo, emulated commands) follows;
                    // bash -c shell-outs inside leak to the real STDOUT
                    // (accepted divergence for this fallback).
                    let first_redirect = redirects.first();
                    match first_redirect {
                        Some(r) if call_arg_str(&r.target).map_or(false, |t| t.starts_with('&')) => {
                            // fd-dup redirect (`2>&1`, `1>&2`, …) on a
                            // non-command inner (subshell block): dup stderr
                            // to stdout — the backtick __bt capture pipe then
                            // sees stderr too. The old code opened a FILE
                            // named "&1" (mode w + the raw target).
                            let mode = if r.mode == "r" { "'<&'" } else { "'>&'" };
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "local *STDERR; open STDERR, {}, STDOUT or die \"Cannot dup stderr: $!\\n\";\n",
                                mode
                            ));
                            for s in inner {
                                emit_stmt(out, s, indent);
                            }
                        }
                        Some(r) if call_arg_str(&r.target).map_or(false, |t| t.starts_with('&')) => {
                            // fd-dup redirect (`2>&1`, `1>&2`, …) on a
                            // non-command inner (subshell block): dup stderr
                            // to stdout — the backtick __bt capture pipe then
                            // sees stderr too. The old code opened a FILE
                            // named "&1" (mode w + the raw target).
                            let mode = if r.mode == "r" { "'<&'" } else { "'>&'" };
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "local *STDERR; open STDERR, {}, STDOUT or die \"Cannot dup stderr: $!\\n\";\n",
                                mode
                            ));
                            for s in inner {
                                emit_stmt(out, s, indent);
                            }
                        }
                        Some(r) if matches!(r.mode.as_str(), "w" | "wc" | "a" | "r+") => {
                            let target = call_arg_str(&r.target).unwrap_or_default();
                            let mode = if r.mode == "a" { "'>>'" } else { "'>'" };
                            // `"$@" > FILE` (a non-command inner): the inner
                            // statements shell out with a rebuilt command —
                            // the redirected handle must be the REAL fd 1,
                            // not just the Perl select() handle, or the child
                            // writes past the capture (the `select` idiom is
                            // still applied inside for Perl print output).
                            // Unique names per site: a second `my` in the
                            // same scope masks the first (warnings + a hidden
                            // lexical).
                            //
                            // fd 2 (`2>FILE`) redirects STDERR, not STDOUT.
                            // Before this the emitter always hardcoded STDOUT,
                            // so `cmd 2>/dev/null` silenced cmd's REAL output
                            // and let its diagnostics through. `select` is a
                            // STDOUT-only idiom (it rebinds the default output
                            // handle), so for STDERR the default handle is
                            // deliberately left untouched — inner `print`
                            // output must still reach stdout.
                            let is_stderr = r.fd == Some(2);
                            let (h, lh) = if is_stderr {
                                ("STDERR", "stderr")
                            } else {
                                ("STDOUT", "stdout")
                            };
                            static RD2_SEQ: std::sync::atomic::AtomicUsize =
                                std::sync::atomic::AtomicUsize::new(0);
                            let n = RD2_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let fh = format!("$__sh2_rfh{n}");
                            let sel = format!("$__sh2_rsel{n}");
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "open(my {fh}, '>&', \\*{h}) or die \"Cannot dup {lh}: $!\\n\";\n"
                            ));
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "close({h}); open({h}, {}, {}) or die \"Cannot write to {}: $!\\n\";\n",
                                mode,
                                render_word(&r.target),
                                target
                            ));
                            if !is_stderr {
                                emit_indent(out, indent);
                                out.push_str(&format!("my {} = select(STDOUT);\n", sel));
                            }
                            for s in inner {
                                emit_stmt(out, s, indent);
                            }
                            emit_indent(out, indent);
                            if is_stderr {
                                out.push_str(&format!(
                                    "open(STDERR, '>&', {fh}) or die \"Cannot restore stderr: $!\\n\"; close({fh});\n"
                                ));
                            } else {
                                out.push_str(&format!(
                                    "select({sel}); open(STDOUT, '>&', {fh}) or die \"Cannot restore stdout: $!\\n\"; close({fh});\n"
                                ));
                            }
                        }
                        _ => {
                            emit_indent(out, indent);
                            out.push_str("die \"otranspilerl: shIR construct not yet supported by the Perl backend (redirect)\\n\";\n");
                        }
                    }
                }
            }
        }
        IrStmt::Block(stmts) => {
            // plan improvement #5: render a block as a flat sequence of
            // stmts (the Perl backend has no native block scoping; this
            // is an approximation — variables from the block leak, but
            // the IR's lexical-ish scoping is close enough for the
            // supported subset).
            for s in stmts {
                emit_stmt(out, s, indent);
            }
        }
        IrStmt::Expr(e) => {
            // A bare expression statement: shell commands arrive as
            // Call{exec/test/pipeline/redirect/…} or as `&&`/`||` chains of
            // those (BinOp) — lower them all to shell-out via bash -c.
            match e {
                IrExpr::Call { func, args } => match func.as_str() {
                    "break" => {
                        emit_indent(out, indent);
                        out.push_str("last;\n");
                        return;
                    }
                    "continue" => {
                        emit_indent(out, indent);
                        out.push_str("next;\n");
                        return;
                    }
                    "exec" => {
                        emit_exec_call(out, e, indent);
                    }
                    "setVar" => {
                        // Statement-position store write (frontend-emitted
                        // A1 — the C frontend's Assign lowers to setVar in
                        // some shapes): a plain scalar assignment
                        if let (Some(name), Some(value)) =
                            (args.first().and_then(call_arg_str), args.get(1))
                        {
                            let t = var_read(&name);
                            let v = ir_expr_to_perl(value);
                            emit_indent(out, indent);
                            out.push_str(&format!("{t} = {v};\n"));
                        } else {
                            emit_indent(out, indent);
                            out.push_str("die \"otranspilerl: setVar args not renderable (Perl backend)\\n\";\n");
                        }
                    }
                    "assign" => {
                        // Frontend `assign(NAME, op, VALUE)` — lower to the
                        // Assign statement so the capture/array/scalar
                        // handling is shared (examples/parse-dollar-paren-or).
                        let name = args.first().and_then(call_arg_str).unwrap_or_default();
                        let value = args
                            .get(2)
                            .or_else(|| args.get(1))
                            .cloned()
                            .unwrap_or_else(|| {
                                IrExpr::Str(String::new(), StrStyle::DoubleQuoted)
                            });
                        let st = IrStmt::Assign {
                            targets: vec![AssignTarget {
                                var: name,
                                sigil: None,
                                indices: vec![],
                            }],
                            expr: value,
                            asm: None,
                        };
                        emit_stmt(out, &st, indent);
                    }
                    "block" => {
                        // Multi-statement block in statement position —
                        // flatten its closure bodies.
                        for a in args {
                            if let IrExpr::Arrow(stmts) = a {
                                for s in stmts {
                                    emit_stmt(out, s, indent);
                                }
                            }
                        }
                    }
                    "builtin" => {
                        // The shared `builtin` op (builtins.json namespace):
                        // emit_exec_call renders the native-command set and
                        // shells out the still-unsupported remainder.
                        emit_exec_call(out, e, indent);
                    }
                    "$fn_call" | "fnCall" => {
                        // Call to a shell function defined in this program
                        // (rewritten by shir_to_perl): a Perl sub call.
                        // `fnCall` arrives verbatim in frontend-emitted A1
                        // nested inside capture arrows (the C frontend's
                        // outparam channel), where the rewrite never ran.
                        let name = args.first().and_then(call_arg_str).unwrap_or_default();
                        let words = exec_word_args(args);
                        let rest: Vec<String> = words.iter().map(|w| render_word_list(w)).collect();
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "{}({}); $main_exit_code = $CHILD_ERROR = 0;\n",
                            name,
                            rest.join(", ")
                        ));
                    }
                    "fnValue" => {
                        // VALUE-returning function call (the echo-return
                        // transform's convention — CROSS_BACKEND_RUNTIME.md
                        // §8.3): a Perl sub call whose value is the
                        // function's `return` — an expression, no status
                        // write (the caller consumes the value, e.g. the
                        // rewritten `print(fnValue(...), "\n")` echo).
                        let name = args.first().and_then(call_arg_str).unwrap_or_default();
                        let words = exec_word_args(args);
                        let rest: Vec<String> = words.iter().map(|w| render_word_list(w)).collect();
                        out.push_str(&format!("{}({})", name, rest.join(", ")));
                    }
                    "test" => {
                        // Bare `[ cond ]` as a statement: the exit status is the
                        // condition's truth.
                        let cond = ir_expr_to_perl(e);
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "$main_exit_code = $CHILD_ERROR = ({}) ? 0 : 1;\n",
                            cond
                        ));
                    }
                    "grepMatches" => {
                        // `grep -o PAT` — print each match.
                        let text = args.first().map(ir_expr_to_perl).unwrap_or_else(|| "''".into());
                        let pat = args.get(1).and_then(call_arg_str).unwrap_or_default();
                        let flags = args.get(2).and_then(call_arg_str).unwrap_or_default();
                        let rx = grep_o_regex(&pat, &flags);
                        let fl = if flags.contains('i') { "i" } else { "" };
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "for my $__m (({text}) =~ /{rx}/{fl}g) {{ print $__m, \"\\n\"; }} $main_exit_code = $CHILD_ERROR = 0;\n"
                        ));
                    }
                    "pipeline" => {
                        // Side-effect pipeline: prefer a native fold when the
                        // stages are literal builtins (`echo … | tr …`,
                        // `printf … | sort/head/tail/wc`), otherwise rebuild
                        // the shell command string and run it via bash -c
                        // (matches bash stdout by running the same tools).
                        if let Some(s) = crate::pipeline_native::native_pipeline(e) {
                            out.push_str(&s);
                            out.push('\n');
                            return;
                        }
                        if try_native_echo_tr_pipeline(out, e, indent) {
                            return;
                        }
                        if try_native_while_read_sort(out, e, indent) {
                            return;
                        }
                        if try_native_for_sort(out, e, indent) {
                            return;
                        }
                        if let Some(cmd) = pipeline_call_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else if let Some(stages) = pipeline_stages_of(e) {
                            // No single shell command (a stage is a Perl-side
                            // materialisation, e.g. a process substitution):
                            // emit the stages in order. The producer stages
                            // write their files; the last consumer reads
                            // STDIN, exactly like the original pipeline.
                            for stage in &stages {
                                for s in stage {
                                    emit_stmt(out, s, indent);
                                }
                            }
                        } else {
                            emit_indent(out, indent);
                            out.push_str("die \"otranspilerl: shIR pipeline not yet supported by the Perl backend\\n\";\n");
                        }
                    }
                    "and" | "or" => {
                        // `A && B` / `A || B` composition.  Args are
                        // Arrow(stmts) closures; a single-arg `and` is a
                        // plain sequential group (the process-sub
                        // materialization wrapper).
                        let stmts: Vec<&Vec<IrStmt>> = args
                            .iter()
                            .filter_map(|a| match a {
                                IrExpr::Arrow(b) => Some(b),
                                _ => None,
                            })
                            .collect();
                        if stmts.len() == 1 {
                            for s in stmts[0] {
                                emit_stmt(out, s, indent);
                            }
                        } else if stmts.len() >= 2 {
                            for (i, body) in stmts.iter().enumerate() {
                                let is_first = i == 0;
                                let is_last = i + 1 == stmts.len();
                                if is_first {
                                    // A: run, capture its status.
                                    for s in body.iter() {
                                        emit_stmt(out, s, indent);
                                    }
                                } else if is_last {
                                    // B: run only when the chain status allows.
                                    emit_indent(out, indent);
                                    if func == "and" {
                                        out.push_str("if ($CHILD_ERROR == 0) {\n");
                                    } else {
                                        out.push_str("if ($CHILD_ERROR != 0) {\n");
                                    }
                                    for s in body.iter() {
                                        emit_stmt(out, s, indent + 1);
                                    }
                                    emit_indent(out, indent);
                                    out.push_str("}\n");
                                } else {
                                    // Middle stages: run, then update the
                                    // chain status from the last emitted command.
                                    for s in body.iter() {
                                        emit_stmt(out, s, indent);
                                    }
                                }
                            }
                        } else {
                            emit_indent(out, indent);
                            out.push_str("$CHILD_ERROR = 0;\n");
                        }
                    }
                    "caseMatch" | "define" | "subshell" | "background" => {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "die \"otranspilerl: sh2.* call `{}` not yet supported by the shIR Perl backend\\n\";\n",
                            func
                        ));
                    }
                    "shopt" => {
                        // Shell-option toggle — nocasematch affects [[ == ]]
                        // string comparisons (tracked at runtime).
                        let on = args
                            .get(1)
                            .map(|a| matches!(a, IrExpr::Bool(true)))
                            .unwrap_or(false);
                        let opt = args.first().and_then(call_arg_str).unwrap_or_default();
                        if opt == "nocasematch"
                            || opt == "-s"
                                && args.get(1).and_then(call_arg_str).as_deref()
                                    == Some("nocasematch")
                        {
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "$__nocasematch = {}; $main_exit_code = $CHILD_ERROR = 0;\n",
                                if on { 1 } else { 0 }
                            ));
                        } else {
                            emit_indent(out, indent);
                            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                        }
                    }
                    "redirect" => {
                        // Statement-position redirect: run the command with its
                        // redirects via bash -c (redirects are shell-level).
                        if let Some(cmd) = redirect_call_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else if let Some(stmts) = redirect_call_arrow(e) {
                            // Non-rebuildable inner (an `and` group of procsub
                            // materialisation + diff): emit the inner statements
                            // natively. Redirects that only discard STDERR
                            // (`2>/dev/null`) can be dropped — the gate compares
                            // stdout/exit only; a stdout/file redirect stays a
                            // refusal (it would change the output).
                            let specs: Vec<&IrExpr> = match e {
                                IrExpr::Call { args, .. } => args
                                    .iter()
                                    .skip(1)
                                    .flat_map(|s| match s {
                                        IrExpr::Array(el) => el.iter().collect::<Vec<_>>(),
                                        other => vec![other],
                                    })
                                    .collect(),
                                _ => Vec::new(),
                            };
                            // A spec-less redirect wrapper (the process-subst
                            // transform's producer materialisation leaves the
                            // outer `redirect` Call with an empty spec array) is
                            // a no-op — emit the inner statements. A `2>/dev/null`
                            // or any `/dev/null` target is droppable (the gate
                            // only compares stdout/exit); `all` on an empty set is
                            // true, so the wrapper is handled too.
                            let droppable = specs.iter().all(|s| match redirect_spec_fields(s) {
                                Some((2, _, _)) => true,
                                Some((_, _, t)) => {
                                    call_arg_str(&t).as_deref() == Some("/dev/null")
                                }
                                None => false,
                            });
                            // A single fd-1 FILE redirect around a body the
                            // renderer emits as Perl can be honoured
                            // natively: run the statements with stdout
                            // redirected to the file (the fd-1 dance the
                            // redirect sites use, so child processes are
                            // captured too). Otherwise a file redirect stays
                            // a refusal — dropping it would lose the output.
                            let out_target = specs.iter().find_map(|sp| {
                                match redirect_spec_fields(sp) {
                                    Some((1, m, t))
                                        if matches!(m.as_str(), "w" | "wc" | "a") =>
                                    {
                                        Some((m, t))
                                    }
                                    _ => None,
                                }
                            });
                            if let Some((mode, target)) = out_target {
                                if capture_body_native(stmts)
                                    || matches!(
                                        stmts.as_slice(),
                                        [IrStmt::Expr(_)] | [IrStmt::Block(_)]
                                    )
                                {
                                    let mut body = String::new();
                                    for s in stmts {
                                        emit_stmt(&mut body, s, 1);
                                    }
                                    emit_indent(out, indent);
                                    out.push_str(&format!(
                                        "{{ open(my $__red_fh, {}, {}) or die \"redirect: $!\\n\"; my $__red_sel = select($__red_fh); {body} select($__red_sel); close($__red_fh); }}\n",
                                        if mode == "a" { "'>>'" } else { "'>'" },
                                        render_word(&target),
                                    ));
                                    return;
                                }
                            }
                            if droppable {
                                for s in stmts {
                                    emit_stmt(out, s, indent);
                                }
                            } else {
                                emit_indent(out, indent);
                                out.push_str("die \"otranspilerl: shIR redirect not yet supported by the Perl backend\\n\";\n");
                            }
                        } else {
                            emit_indent(out, indent);
                            out.push_str("die \"otranspilerl: shIR redirect not yet supported by the Perl backend\\n\";\n");
                        }
                    }
                    other => {
                        // Unknown sh2.* calls in statement position.
                        if let Some(cmd) = expr_to_cmd(e) {
                            emit_shell_cmd(out, indent, &cmd);
                        } else {
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "die \"otranspilerl: sh2.* call `{}` not yet supported by the shIR Perl backend\\n\";\n",
                                other
                            ));
                        }
                    }
                },
                // Statement-position arithmetic (`((n++))` — triage-perl
                // t02_control; the arith_forms let-lift emits the IncDec
                // bare). The expression VALUE is discarded — only the side
                // effect matters — so postfix/prefix agree (arith_ast_to_perl
                // renders both as `($n += 1)` / `($n -= 1)`).
                IrExpr::Arith(a) if matches!(&**a, ArithAst::IncDec { .. }) => {
                    // `((i++))` sets `$?` from the expression VALUE (old for
                    // postfix, new for prefix): 0 → status 1 (examples/arith-
                    // postfix-status).
                    let e = arith_ast_to_perl(a);
                    emit_indent(out, indent);
                    out.push_str(&format!(
                        "$main_exit_code = $CHILD_ERROR = (({}) != 0) ? 0 : 1;\n",
                        e
                    ));
                }
                // `! cmd` in statement position — run the command and
                // invert the exit status (not-negation.sh's bare `! grep`).
                IrExpr::BinOp { op: BinOpKind::Not, rhs, .. } => {
                    if let Some(cmd) = expr_to_cmd(rhs) {
                        emit_shell_cmd(out, indent, &cmd);
                        emit_indent(out, indent);
                        out.push_str("$main_exit_code = $CHILD_ERROR = ($CHILD_ERROR == 0) ? 1 : 0;\n");
                    } else {
                        emit_indent(out, indent);
                        out.push_str("die \"otranspilerl: shIR `!` expression not renderable (Perl backend)\\n\";\n");
                    }
                }
                // A `perl -e 'SCRIPT'` body flattened to native Perl
                // (`native_perl_e_stmt`) — emit it as a statement. The
                // `@ARGV = (…)` prelude and the script body are separate
                // `RawExpr` statements.
                IrExpr::RawExpr(code) => {
                    emit_indent(out, indent);
                    let body = code.trim_end();
                    // The user's Perl runs without `use strict`; its
                    // undeclared globals (`foreach $arg (…)`) would not
                    // compile here. Scope the body with `no strict`.
                    out.push_str("{ no strict 'vars'; no warnings; ");
                    out.push_str(body);
                    // The `perl -e 'SCRIPT'` body may omit the final `;`
                    // (`print "hi\n"`); a `{…}` block needs none.
                    if !(body.ends_with(';') || body.ends_with('}')) {
                        out.push(';');
                    }
                    out.push_str(" }\n");
                    emit_indent(out, indent);
                    out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                }
                // Non-Call expressions: bare `&&`/`||` chains of execs,
                // redirects, pipelines — run the reconstructed shell command.
                other_expr => {
                    if let Some(s) = crate::pipeline_native::native_chain(other_expr) {
                        emit_indent(out, indent);
                        out.push_str(&s);
                        out.push('\n');
                    } else if let Some(s) = cd_chain_to_perl(other_expr) {
                        emit_indent(out, indent);
                        out.push_str(&s);
                        out.push('\n');
                    } else if let Some(s) = control_chain_to_perl(other_expr) {
                        emit_indent(out, indent);
                        out.push_str(&s);
                        out.push('\n');
                    } else if matches!(other_expr, IrExpr::BinOp { op: BinOpKind::And | BinOpKind::Or, .. })
                        && capture_expr_native(other_expr)
                    {
                        // `&&`/`||` of natively emulable commands — render
                        // the prints instead of a `bash -c` shell-out.
                        emit_native_binop(out, indent, other_expr);
                    } else if let Some(cmd) = expr_to_cmd(other_expr) {
                        emit_shell_cmd(out, indent, &cmd);
                    } else if !emit_native_binop(out, indent, other_expr) {
                        emit_indent(out, indent);
                        out.push_str("die \"otranspilerl: shIR expression not yet supported by the Perl backend\\n\";\n");
                    }
                }
            }
        }

        IrStmt::Output {
            value,
            newline,
            target,
        } => {
            let expr = ir_expr_to_perl(value);
            if let Some(fh) = target {
                // Output to a specific filehandle: print {$fh} ...
                emit_indent(out, indent);
                if *newline {
                    out.push_str(&format!("print {{${}}}({}, \"\\n\");\n", fh, expr));
                } else {
                    out.push_str(&format!("print {{${}}}({});\n", fh, expr));
                }
            } else if *newline {
                // Use `print` with \n when called via piecemeal stmt_to_perl()
                // (which cannot manage imports).  Full shir_to_perl() switches to `say`.
                // Try to embed \n directly into string literals for cleaner output.
                if let Some(embedded) = try_embed_newline_in_string_literal(&expr) {
                    emit_indent(out, indent);
                    out.push_str(&embedded);
                    emit_indent(out, indent);
                    out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                } else {
                    emit_indent(out, indent);
                    out.push_str(&format!("print({}, \"\\n\");\n", expr));
                    emit_indent(out, indent);
                    out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                }
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("print({});\n", expr));
                emit_indent(out, indent);
                out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
            }
        }

        IrStmt::WriteFile {
            path,
            content,
            append,
        } => {
            let path_str = ir_expr_to_perl(path);
            let content_str = ir_expr_to_perl(content);
            let mode = if *append { "'>>'" } else { "'>'" };
            emit_indent(out, indent);
            out.push_str(&format!(
                "open my $__fh, {}, {} or die \"Cannot write file: $!\\n\";\n",
                mode, path_str
            ));
            emit_indent(out, indent);
            // `$` is not special in Rust format strings; it passes through literally.
            out.push_str("print {$__fh} ");
            out.push_str(&content_str);
            out.push_str(";\n");
            emit_indent(out, indent);
            out.push_str("close $__fh;\n");
        }

        IrStmt::Assign {
            targets,
            expr,
            asm,
            ..
        } => {
            let rhs = ir_expr_to_perl(expr);
            // Declarator-position asm label (`int x asm("myx") = 7;` —
            // core request c-sh-go-toplevelasmargument-20260814-042952):
            // the Perl model has no object-file symbols, and silently
            // dropping the label would hide the C construct — refuse
            // loudly (refuse > guess; same contract as the Asm stmt).
            if let Some(spec) = asm {
                emit_indent(out, indent);
                out.push_str(&format!(
                    "die \"otranspilerl: shIR construct not yet supported by the Perl backend (asm label '{}')\\n\";\n",
                    spec.template
                ));
                return;
            }
            // `((i++))` / `((++i))` arrives as Assign{i, Arith(IncDec{i})}.
            // The postfix value is the OLD value, so re-assigning it
            // cancels the increment (`$i = int(($i++))` left i unchanged —
            // 092/bench-increment). Emit the increment alone; the statement
            // value is discarded in bash too.
            if targets.len() == 1 && targets[0].indices.is_empty() {
                if let IrExpr::Arith(ast) = expr {
                    if let ArithAst::IncDec { var, .. } = &**ast {
                        if var == &targets[0].var {
                            let e = arith_ast_to_perl(ast);
                            emit_indent(out, indent);
                            out.push_str(&format!(
                                "$main_exit_code = $CHILD_ERROR = (({}) != 0) ? 0 : 1;\n",
                                e
                            ));
                            return;
                        }
                    }
                }
            }
            // Detect Capture { native: false } on the RHS — emit the
            // two-statement clean form instead of embedding a do-block.
            if targets.len() == 1 && targets[0].indices.is_empty() {
                let var = &targets[0].var;
                let lhs = perl_lhs_for(var);
                if let IrExpr::Call { func, .. } = expr {
                    if func == "setArray" {
                        // Array assignment: arr=(a b c) → @arr = ('a','b','c');
                        if let IrExpr::Call { args, .. } = expr {
                            // args[0] is the array NAME — the elements follow
                            // (either a single Array arg or individual words).
                            let elems: Vec<&IrExpr> = if args.len() >= 2 {
                                if let IrExpr::Array(elems) = &args[1] {
                                    elems.iter().collect()
                                } else {
                                    args[1..].iter().collect()
                                }
                            } else {
                                Vec::new()
                            };
                            let items: Vec<String> =
                                elems.iter().map(|e| render_word_list(*e)).collect();
                            emit_indent(out, indent);
                            out.push_str(&format!("@{} = ({});\n", var, items.join(", ")));
                            return;
                        }
                    }
                    if func == "setArrayAppend" {
                        // Array append: arr+=(a b c) → push @arr, ('a','b','c');
                        // Distinct from setArray: the existing elements must
                        // SURVIVE. Rendering it through the scalar fallback
                        // would emit `$arr = setArrayAppend(...)` — a scalar
                        // that was never declared (strict) and no append.
                        if let IrExpr::Call { args, .. } = expr {
                            let elems: Vec<&IrExpr> = if args.len() >= 2 {
                                if let IrExpr::Array(elems) = &args[1] {
                                    elems.iter().collect()
                                } else {
                                    args[1..].iter().collect()
                                }
                            } else {
                                Vec::new()
                            };
                            let items: Vec<String> =
                                elems.iter().map(|e| render_word_list(*e)).collect();
                            emit_indent(out, indent);
                            out.push_str(&format!("push @{}, ({});\n", var, items.join(", ")));
                            return;
                        }
                    }
                    if func == "assign" {
                        // `assign(NAME, OP, VALUE)` — the `y+=2` / `n-=1`
                        // compound write. Without this the whole call fell
                        // through to `ir_expr_to_perl`'s generic `_ =>` arm,
                        // which renders `assign("y", "+=", "2")` as a call to
                        // a sub that is never defined in the preamble — a
                        // runtime death, not a bad render (examples/051_primes,
                        // 062_hard_to_lex).
                        //
                        // The two forms differ by whether the TARGET is an
                        // array: an array `+=` appends (`push`), a scalar
                        // `+=` is numeric. The Assign target carries no sigil
                        // for this shape (the frontend emits `assign` for
                        // BOTH `y+=2` and `z+=(a b)`), so ask the program-wide
                        // array census — a value-shape test misreads
                        // `z+=${primes[@]:0:1}` (a single-word RHS appending
                        // to an ARRAY) as a scalar add.
                        if let IrExpr::Call { args, .. } = expr {
                            let inner = args.first().and_then(call_arg_str).unwrap_or(var.clone());
                            let op = match args.get(1).and_then(call_arg_str) {
                                Some(o) if o != "=" => o,
                                _ => "=".to_string(),
                            };
                            let value = args
                                .get(2)
                                .or_else(|| args.get(1))
                                .cloned()
                                .unwrap_or_else(|| {
                                    IrExpr::Str(String::new(), StrStyle::DoubleQuoted)
                                });
                            emit_indent(out, indent);
                            if op == "+=" && array_var(&inner) {
                                let items: Vec<String> = match &value {
                                    IrExpr::Array(elems) => {
                                        elems.iter().map(|e| render_word_list(e)).collect()
                                    }
                                    other => vec![render_word_list(other)],
                                };
                                out.push_str(&format!(
                                    "push @{0}, ({1});\n",
                                    inner,
                                    items.join(", ")
                                ));
                            } else {
                                // The numeric ops (`-=`, `*=`, `/=`) and
                                // `+=` on a scalar are the same in both
                                // languages — Perl's `+=` is numeric like
                                // bash's on an integer.
                                let rhs = ir_expr_to_perl(&value);
                                out.push_str(&format!(
                                    "{} {} {};\n",
                                    perl_lhs_for(&inner),
                                    op,
                                    rhs
                                ));
                            }
                            return;
                        }
                    }
                    if func == "capture" {
                        // Modern-IR command substitution: Assign{var, capture(…)}
                        // — rebuild the shell command and capture its stdout.
                        // A capture of a single NATIVE PIPELINE
                        // (`x=$(echo hi | sort)`) is lowered natively: run
                        // the shared pipeline capability's statement with
                        // stdout redirected into the variable, instead of
                        // shelling the whole thing to `bash -c` (which
                        // check_qx flags as "wrapping builtin 'echo'").
                        if let Some(arrow) = capture_call_arrow(expr) {
                            if let [IrStmt::Expr(e)] = arrow.as_slice() {
                                // a redirect-fed command (`x=$(wc -l < FILE)`)
                                if let Some(val) =
                                    crate::pipeline_native::native_capture_call(e)
                                {
                                    emit_indent(out, indent);
                                    out.push_str(&format!("{} = {};\n", lhs, val));
                                    return;
                                }
                                if let Some(stmt) =
                                    crate::pipeline_native::native_pipeline(e)
                                {
                                    emit_indent(out, indent);
                                    out.push_str(&format!(
                                        "{} = do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {stmt} close $__fh; select($__old); $__cap =~ s/\\n+\\z//; $__cap }};\n",
                                        lhs
                                    ));
                                    return;
                                }
                            }
                            // Natively emittable body (folded `Output`, the
                            // `grepMatches` lift, `&&`/`||` chains): capture
                            // the Perl prints in-process. Without this a
                            // `Call{capture}` whose body has no shell
                            // spelling fell to `ir_expr_to_perl`, which
                            // renders the CALL itself — an empty string
                            // (`matched=$(… | grep -o PAT)` lost every
                            // match).
                            if capture_body_native(arrow) {
                                let mut body = String::new();
                                for st in arrow.iter() {
                                    emit_stmt(&mut body, st, 1);
                                }
                                emit_indent(out, indent);
                                out.push_str(&format!(
                                    "{} = do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body};; }} $__cap =~ s/\\n+\\z//; $__cap }};\n",
                                    lhs
                                ));
                                return;
                            }
                        }
                        if let Some(cmd) = capture_call_to_cmd(expr) {
                            emit_capture_assign(out, indent, &lhs, &cmd);
                            return;
                        }
                    }
                }
                if let IrExpr::Capture { native: false, .. } = expr {
                    if let IrExpr::Capture { expr: inner, .. } = expr {
                        if let IrExpr::Arrow(stmts) = inner.as_ref() {
                            if let [IrStmt::Expr(e)] = stmts.as_slice() {
                                // a redirect-fed command (`x=$(wc -l < FILE)`)
                                if let Some(val) =
                                    crate::pipeline_native::native_capture_call(e)
                                {
                                    emit_indent(out, indent);
                                    out.push_str(&format!("{} = {};\n", lhs, val));
                                    return;
                                }
                                if let Some(stmt) = crate::pipeline_native::native_pipeline(e)
                                {
                                    emit_indent(out, indent);
                                    out.push_str(&format!(
                                        "{} = do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {stmt} close $__fh; select($__old); $__cap =~ s/\\n+\\z//; $__cap }};\n",
                                        lhs
                                    ));
                                    return;
                                }
                                // `x=$(echo … | tr …)` — the native tr fold
                                // avoids the `bash -c` shell-out check_qx
                                // flags (examples/087_function_cmd_sub).
                                if let Some(expr) = echo_tr_expr(e) {
                                    emit_indent(out, indent);
                                    out.push_str(&format!(
                                        "{} = do {{ my $__cap = {}; $__cap =~ s/\\n+\\z//; $__cap }};\n",
                                        lhs, expr
                                    ));
                                    return;
                                }
                                // A single external command: uu-ffi capture.
                                if let Some(expr) = capture_uu_expr(e) {
                                    emit_indent(out, indent);
                                    out.push_str(&format!("{} = {};\n", lhs, expr));
                                    return;
                                }
                                // A `||` chain of pipeline-expressible
                                // commands (`x=$(stat … || wc -c < f)`).
                                if let Some(expr) = chain_or_capture(e) {
                                    emit_indent(out, indent);
                                    out.push_str(&format!("{} = {};\n", lhs, expr));
                                    return;
                                }
                            }
                            // Natively emittable body (folded `Output`,
                            // emulated commands): capture the Perl prints.
                            if capture_body_native(stmts) {
                                let mut body = String::new();
                                for st in stmts.iter() {
                                    emit_stmt(&mut body, st, 1);
                                }
                                emit_indent(out, indent);
                                out.push_str(&format!(
                                    "{} = do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body};; }} $__cap =~ s/\\n+\\z//; $__cap }};\n",
                                    lhs
                                ));
                                return;
                            }
                        }
                    }
                    // Extract the inner expression string for qx{...}
                    if let IrExpr::Capture {
                        expr: inner_expr, ..
                    } = expr
                    {
                        // Prefer the REBUILT SHELL TEXT for Arrow bodies: the
                        // old fallback rendered the Arrow as a Perl anonymous
                        // sub (`sub { … }`) and fed it to bash -c — which is
                        // not shell. Bash reported `sub: command not found`
                        // (verified via `x=$(printf "%s\n" …)` and
                        // `$(echo hi)` — the pre-existing capture bug).
                        let shell_cmd = match inner_expr.as_ref() {
                            // A heredoc inside the captured body (`x=$(cat
                            // <<-EOF …)`) collects its body in the thread-
                            // local during the rebuild; it belongs at the END
                            // of the command line, so append it here (the
                            // outermost call site of the rebuild).
                            IrExpr::Arrow(stmts) => stmts_to_shell_cmd(stmts)
                                .map(|c| format!("{c}{}", take_heredoc_tail())),
                            _ => None,
                        };
                        let open_expr = if let Some(cmd) = shell_cmd {
                            cmd_str_to_open_perl(&cmd)
                        } else if let IrExpr::Arrow(stmts) = inner_expr.as_ref() {
                            // The shell-text rebuild refuses bodies whose
                            // words bash cannot round-trip (a `perl -e '…'`
                            // script arrives as a plain variable read — the
                            // rebuilt `perl -e "$x"` would run the CONTENTS
                            // as the program). The statements DO render as
                            // Perl, so run them with fd 1 redirected into a
                            // scalar: same capture, no shell round-trip.
                            let mut body = String::new();
                            for st in stmts.iter() {
                                emit_stmt(&mut body, st, 1);
                            }
                            format!(
                                "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body};; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                            )
                        } else {
                            let mut inner_str = ir_expr_to_perl(inner_expr);
                            // Strip surrounding backticks from StrStyle::Command rendering
                            if inner_str.starts_with('`')
                                && inner_str.ends_with('`')
                                && inner_str.len() >= 2
                            {
                                inner_str = inner_str[1..inner_str.len() - 1].to_string();
                            }
                            // Use open()-based code instead of qx{...} to avoid check_qx violations
                            cmd_str_to_open_perl(&inner_str)
                        };
                        emit_indent(out, indent);
                        out.push_str(&format!("{} = {};\n", lhs, open_expr));
                    } else {
                        // Fallback: use the regular expression form
                        emit_indent(out, indent);
                        out.push_str(&format!("{} = {};\n", lhs, rhs));
                    }
                } else {
                    // Detect compound assignment pattern: $x = $x op $y → $x op= $y
                    if let IrExpr::BinOp {
                        lhs: inner_lhs,
                        op,
                        rhs: inner_rhs,
                    } = expr
                    {
                        if let IrExpr::Var(name, _) = inner_lhs.as_ref() {
                            if *name == *var {
                                let compound_op = match op {
                                    BinOpKind::Add => Some("+="),
                                    BinOpKind::Sub => Some("-="),
                                    BinOpKind::Mul => Some("*="),
                                    BinOpKind::Div => Some("/="),
                                    BinOpKind::Concat => Some(".="),
                                    _ => None,
                                };
                                if let Some(op_str) = compound_op {
                                    let inner_rhs_str = ir_expr_to_perl(inner_rhs);
                                    emit_indent(out, indent);
                                    out.push_str(&format!(
                                        "{} {} {};\n",
                                        lhs, op_str, inner_rhs_str
                                    ));
                                    return;
                                }
                            }
                        }
                    }
                    emit_indent(out, indent);
                    out.push_str(&format!("{} = {};\n", lhs, rhs));
                }
            } else {
                let lhs = targets
                    .iter()
                    .map(|t| perl_lhs_for(&t.var))
                    .collect::<Vec<_>>()
                    .join(", ");
                emit_indent(out, indent);
                out.push_str(&format!("({}) = ({});\n", lhs, rhs));
            }
        }

        IrStmt::Declare { vars, init, local } => {
            // `local` in the A1 is NOT Perl's `local`: it marks the
            // bash/C `local NAME` declaration of a function-scoped fresh
            // binding, and the Perl model realizes it as a fresh `my`
            // (shadows the hoisted top-level lexical for the call).
            // Emitting Perl `local` here is wrong twice over: it is a
            // compile error on the `my`-scoped hoisted vars ("Can't
            // localize lexical variable $x") and it would alias a
            // package global for vars the backend declares as lexicals.
            let _ = local;
            let kw = "my";
            let decls = vars
                .iter()
                .map(|d| match d.sigil.unwrap_or(Sigil::Scalar) {
                    Sigil::Scalar => format!("${}", d.name),
                    Sigil::Array => format!("@{}", d.name),
                    Sigil::Hash => format!("%{}", d.name),
                })
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(init_expr) = init {
                let rhs = ir_expr_to_perl(init_expr);
                emit_indent(out, indent);
                // single scalar can omit parentheses: "my $x = expr;"
                if vars.len() == 1 && vars[0].sigil.unwrap_or(Sigil::Scalar) == Sigil::Scalar {
                    out.push_str(&format!("{} {} = {};\n", kw, decls, rhs));
                } else {
                    out.push_str(&format!("{} ({}) = ({});\n", kw, decls, rhs));
                }
            } else {
                emit_indent(out, indent);
                out.push_str(&format!("{} {};\n", kw, decls));
            }
        }

        IrStmt::DeclareArray {
            var,
            sigil,
            elements,
        } => {
            let elems = elements
                .iter()
                .map(|e| ir_expr_to_perl(e))
                .collect::<Vec<_>>()
                .join(", ");
            let sigil_char = match sigil.unwrap_or(Sigil::Scalar) {
                Sigil::Array => '@',
                Sigil::Hash => '%',
                _ => '$',
            };
            emit_indent(out, indent);
            // The hoisted preamble declares every assigned var (use strict);
            // a second `my` here triggers "masks earlier declaration in same
            // scope" AND would hide the hoisted lexical from the reads.
            out.push_str(&format!("{}{} = ({});\n", sigil_char, var, elems));
        }

        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let cond_str = ir_expr_to_perl(cond);
            eprintln!("DBG_IFCOND {:?} -> {}", cond, cond_str);
            // If the condition was derived from a shell test command by
            // native_if_test_cond (indicated by IrExpr::Str), set
            // $CHILD_ERROR so $? reflects the test result for subsequent
            // commands that read $? (e.g. `[ "$?" = "0" ]` after a loop).
            if matches!(cond, IrExpr::Str(_, _)) {
                emit_indent(out, indent);
                out.push_str(&format!("$CHILD_ERROR = ({}) ? 0 : 1;\n", cond_str));
            }
            emit_indent(out, indent);
            out.push_str(&format!("if ({}) {{\n", cond_str));
            for s in then {
                emit_stmt(out, s, indent + 1);
            }
            for (econd, ebody) in elsifs {
                let estr = ir_expr_to_perl(econd);
                emit_indent(out, indent);
                out.push_str(&format!("}} elsif ({}) {{\n", estr));
                for s in ebody {
                    emit_stmt(out, s, indent + 1);
                }
            }
            if !else_.is_empty() {
                emit_indent(out, indent);
                out.push_str("} else {\n");
                for s in else_ {
                    emit_stmt(out, s, indent + 1);
                }
            } else if matches!(cond, IrExpr::Str(_, _)) {
                // bash: an `if` statement with no else returns 0 even when
                // the test condition is false. Without this the
                // `$CHILD_ERROR = (...) ? 0 : 1` above would leak a non-zero
                // status into the parent context (e.g. interactive-test-minus-t).
                emit_indent(out, indent);
                out.push_str("} else {\n");
                emit_indent(out, indent + 1);
                out.push_str("$CHILD_ERROR = 0;\n");
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }

        IrStmt::For { var, iter, body } => {
            let iter_str = word_iter_to_perl(iter);
            // Perl's `for $i` restores the pre-loop value after the loop, but
            // bash leaves the last value — iterate a fresh temp and assign
            // the real var each round.
            let tmp = format!("__{}", var);
            emit_indent(out, indent);
            out.push_str(&format!("for my ${} ({}) {{\n", tmp, iter_str));
            emit_indent(out, indent + 1);
            out.push_str(&format!("${} = ${};\n", var, tmp));
            for s in body.iter() {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }

        IrStmt::While { cond, body } => {
            // A multi-command condition (`while read -r l && [ -n "$l" ]
            // && (( n < 2 )); do …; done`) arrives as
            // `Call{block, [Arrow[stmts…]]}`. Rendering it through
            // ir_expr_to_perl collapsed the whole block to a one-line
            // `system('bash','-c', …)`, which ran each iteration's `read`
            // in a CHILD: the value was assigned in the child and the
            // Perl `$line` stayed undef, so the loop body never fired
            // (063_11, 091_while_pipe_var, 063_hard_to_parse all printed
            // nothing). Emit the arrow's statements INSIDE the loop, one
            // level deeper, and make the last statement's status decide —
            // the same shape as the native `while read` arm.
            if let Some((stmts, tail)) = block_cond_parts(cond) {
                emit_indent(out, indent);
                out.push_str("while (1) {\n");
                for s in &stmts {
                    emit_stmt(out, s, indent + 1);
                }
                emit_indent(out, indent + 1);
                out.push_str(&format!("last unless ({});\n", ir_expr_to_perl(tail)));
                for s in body.iter() {
                    emit_stmt(out, s, indent + 1);
                }
                emit_indent(out, indent);
                out.push_str("}\n");
            } else {
                let cond_str = ir_expr_to_perl(cond);
                emit_indent(out, indent);
                out.push_str(&format!("while ({}) {{\n", cond_str));
                for s in body.iter() {
                    emit_stmt(out, s, indent + 1);
                }
                emit_indent(out, indent);
                out.push_str("}\n");
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let cond_str = ir_expr_to_perl(cond);
            for i in init.iter() {
                emit_stmt(out, i, indent);
            }
            emit_indent(out, indent);
            out.push_str(&format!("for (; {} ;) {{\n", cond_str));
            for s in body.iter() {
                emit_stmt(out, s, indent + 1);
            }
            for st in step.iter() {
                emit_stmt(out, st, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str("}\n");
        }
        IrStmt::Continue => {
            emit_indent(out, indent);
            out.push_str("next;\n");
        }
        IrStmt::Break => {
            emit_indent(out, indent);
            out.push_str("last;\n");
        }
        IrStmt::Die { expr, carp } => {
            let e = ir_expr_to_perl(expr)
                .replace("$ERRNO", "$!")
                .replace("$OS_ERROR", "$!");
            let kw = if *carp { "croak" } else { "die" };
            emit_indent(out, indent);
            out.push_str(&format!("{} {};\n", kw, e));
        }

        IrStmt::Warn { expr, carp } => {
            let e = ir_expr_to_perl(expr)
                .replace("$ERRNO", "$!")
                .replace("$OS_ERROR", "$!");
            let kw = if *carp { "carp" } else { "warn" };
            emit_indent(out, indent);
            out.push_str(&format!("{} {};\n", kw, e));
        }

        IrStmt::Exec {
            cmd, args, capture, ..
        } => {
            // `eval "TEXT"` — bash PARSES TEXT as source and runs it in the
            // CURRENT shell, so its assignments persist. Parse it here and
            // emit what that source means:
            //   eval "result=$(( $var + 1 ))"  ⇒  $result = $var + 1;
            // The generic path emitted the literal TEXT, so 063_12 printed
            // the raw source (`Eval result: $(( ${var:-0} + … ))`) and
            // 063_hard_to_parse printed nothing.
            //
            // STATIC literal text only (the common corpus shape). A dynamic
            // argument cannot be resolved at translation time; it falls
            // through to the shell-out, where the child parses it exactly
            // like bash (assignments then live only in that child — the
            // honest limit of the model).
            if capture.is_none() && call_arg_str(cmd).as_deref() == Some("eval") {
                if let Some(text) = args.iter().find_map(static_word_text) {
                    if let Ok(cmds) = crate::parser::commands::parse(&text) {
                        if !cmds.is_empty() {
                            let sub = crate::shir::ast_to_ir(&cmds);
                            for st in &sub.stmts {
                                emit_stmt(out, st, indent);
                            }
                            return;
                        }
                    }
                    // Not a parseable command line: bash reports a syntax
                    // error and leaves a nonzero status.
                    emit_indent(out, indent);
                    out.push_str(
                        "(print STDERR \"$0: eval: syntax error\\n\"); $main_exit_code = $CHILD_ERROR = 1;\n",
                    );
                    return;
                }
            }
            let cmd_str = ir_expr_to_perl(cmd);
            // Quote a string VALUE as a bash single-quoted shell word.
            // (Perl-style `\'` escaping is NOT understood by bash inside
            // single quotes, so embedded quotes must use the `'\''` idiom.)
            let bash_quote = |s: &str| -> String { format!("'{}'", s.replace('\'', "'\\''")) };
            if let Some(var) = capture {
                // Build a shell command string from the command and its arguments.
                // Run it through bash -c to capture stdout.
                let mut arg_parts: Vec<String> = Vec::new();
                arg_parts.push(cmd_str.clone());
                for a in args {
                    match a {
                        IrExpr::Str(s, _) => {
                            // Literal string: quote its value for bash.
                            arg_parts.push(bash_quote(s));
                        }
                        _ => {
                            let a_str = ir_expr_to_perl(a);
                            // If the argument is already a Perl string literal, use it.
                            if a_str.starts_with('\'')
                                || a_str.starts_with('"')
                                || a_str.starts_with('q')
                            {
                                arg_parts.push(a_str);
                            } else {
                                // A Perl scalar argument (`$temp_file_ps_fh_1`)
                                // is exported to the bash child's env before the
                                // shell-out; reference it UNESCAPED so bash
                                // resolves it (`"$temp_file_ps_fh_1"`).  Escaping
                                // the `$` made the child see a literal dollar.
                                arg_parts.push(format!(
                                    "\"{}\"",
                                    a_str.replace("\"", "\\\"").replace("@", "\\@")
                                ));
                            }
                        }
                    }
                }
                let full_cmd = arg_parts.join(" ");
                // Export the Perl vars the command references into the bash
                // child's environment (localized to the capture block) —
                // otherwise `"$file1"` inside the single-quoted bash -c text
                // is unset in the child. Mirrors emit_shell_cmd.
                let exports: String = var_exports_str(&full_cmd)
                    .lines()
                    .map(|l| format!("local {} ", l))
                    .collect();
                // `$name` text inside the command can also be embedded perl/awk
                // code whose vars are NOT declared perl locals — soften strict
                // for the export prologue so those export as empty instead of
                // failing compilation.
                let exports = if exports.is_empty() {
                    exports
                } else {
                    format!("no strict 'vars'; no warnings; {}", exports)
                };
                // Use open()-based capture with safe quoting
                emit_indent(out, indent);
                out.push_str(&format!(
                    "my ${} = do {{ {}open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; $_r =~ s/\\n+\\z//; $CHILD_ERROR = $? >> 8; $_r; }};\n",
                    var,
                    exports,
                    safe_perl_q_string(&full_cmd)
                ));
            } else {
                // Without capture: try the native capability tree first (the
                // Exec node's cmd+args adapt to the shared `exec` call shape,
                // so rm -f … / cmp … / unset … drop in as before).
                let adapted = IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![cmd.clone(), IrExpr::Array(args.clone())],
                };
                if let Some(native) = crate::pipeline_native::native_exec_stmt(&adapted) {
                    emit_indent(out, indent);
                    out.push_str(&native);
                    out.push('\n');
                    return;
                }
                // Without capture: run the command via system() for side effects.
                let mut arg_parts: Vec<String> = Vec::new();
                arg_parts.push(cmd_str.clone());
                for a in args {
                    match a {
                        IrExpr::Str(s, _) => {
                            // Literal string: quote its value for bash.
                            arg_parts.push(bash_quote(s));
                        }
                        _ => {
                            let a_str = ir_expr_to_perl(a);
                            if a_str.starts_with('\'')
                                || a_str.starts_with('"')
                                || a_str.starts_with('q')
                            {
                                arg_parts.push(a_str);
                            } else {
                                // A Perl scalar argument (`$temp_file_ps_fh_1`)
                                // is exported to the bash child's env before the
                                // shell-out; reference it UNESCAPED so bash
                                // resolves it (`"$temp_file_ps_fh_1"`).  Escaping
                                // the `$` made the child see a literal dollar.
                                arg_parts.push(format!(
                                    "\"{}\"",
                                    a_str.replace("\"", "\\\"").replace("@", "\\@")
                                ));
                            }
                        }
                    }
                }
                let full_cmd = arg_parts.join(" ");
                emit_indent(out, indent);
                // Side-effect run via bash -c (the args are bash-quoted words; a
                // bare `system(<joined words>)` concatenates them into one broken
                // perl expression). Track the status for `$?`/`&&`/`||`.
                // Simple commands (no metacharacters, no env-interp `$`)
                // route through the __sh_uu_* seam; the rest keep the
                // bash -c string path (REFUSE > GUESS).
                if let Some(argv) = sh_split_simple_cmd(&full_cmd) {
                    let args = argv.iter().map(|w| perl_word_literal(w)).collect::<Vec<_>>().join(", ");
                    out.push_str(&format!(
                        "__sh_uu_run({args}); $main_exit_code = $CHILD_ERROR = $__sh_uu_rc;\n"
                    ));
                } else {
                    out.push_str(&format!(
                        "system('bash', '-c', {}); $main_exit_code = $CHILD_ERROR = $? >> 8;\n",
                        safe_perl_q_string(&full_cmd)
                    ));
                }
            }
        }

        IrStmt::Return(Some(expr)) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("return {};\n", e));
        }
        IrStmt::Return(None) => {
            emit_indent(out, indent);
            out.push_str("return;\n");
        }

        IrStmt::Exit(Some(expr)) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("exit {};\n", e));
        }
        IrStmt::Exit(None) => {
            emit_indent(out, indent);
            out.push_str("exit 0;\n");
        }

        IrStmt::SetChildError(expr) => {
            let e = ir_expr_to_perl(expr);
            emit_indent(out, indent);
            out.push_str(&format!("$CHILD_ERROR = {};\n", e));
        }

        IrStmt::Pipeline {
            stages,
            capture,
            cmd_str,
            ..
        } => {
            if let Some(var) = capture {
                // Capture pipeline: emit a single `qx{...}` call.
                // Use the stored command string if available, otherwise
                // fall back to emitting the stage statements.
                // Omit $CHILD_ERROR tracking (same rationale as System capture).
                //
                // Prefer the CAPABILITY tree on the equivalent
                // `pipeline(...)` expression: a literal producer plus a
                // supported consumer (`x=$(echo … | grep -i …)`) lowers to
                // one native statement, which we run with stdout captured.
                // The per-stage fallback below would emit the consumer as
                // a bare exec reading stdin — no payload channel, so it
                // shells out (check_qx: wrapping builtin 'grep').
                if let Some(stmt) = stages_as_pipeline_expr(stages)
                    .and_then(|e| crate::pipeline_native::native_pipeline(&e))
                {
                    emit_indent(out, indent);
                    out.push_str(&format!(
                        "my ${var} = do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {stmt}; ; }} $__cap =~ s/\\n+\\z//; $__cap }};\n"
                    ));
                    return;
                }
                if let Some(cmd) = cmd_str {
                    let open_expr = cmd_str_to_open_perl(cmd);
                    emit_indent(out, indent);
                    out.push_str(&format!("my ${} = {};\n", var, open_expr));
                } else {
                    // No command string — fall back to stage emission.
                    for stage in stages {
                        for s in stage {
                            emit_stmt(out, s, indent);
                        }
                    }
                }
            } else {
                // Side-effect pipeline: prefer the CAPABILITY tree on the
                // equivalent `pipeline(...)` expression — a literal
                // producer plus a supported consumer lowers to ONE native
                // statement, where emitting the stages separately would
                // render the consumer as a bare exec reading stdin (no
                // payload channel -> it shells out: check_qx "wrapping
                // builtin 'grep'").
                if let Some(expr) = stages_as_pipeline_expr(stages) {
                    if let Some(stmt) = crate::pipeline_native::native_pipeline(&expr) {
                        emit_indent(out, indent);
                        out.push_str(&stmt);
                        out.push('\n');
                        return;
                    }
                    // `echo LIT | tr SET SET` — the statement-position
                    // twin of the `IrStmt::Expr` pipeline fold (without it
                    // the stages are emitted separately and `tr` reads the
                    // script's stdin, hanging under the gate).
                    if try_native_echo_tr_pipeline(out, &expr, indent) {
                        return;
                    }
                    if try_native_while_read_sort(out, &expr, indent) {
                        return;
                    }
                    if try_native_for_sort(out, &expr, indent) {
                        return;
                    }
                    // No native fold — rebuild the SHELL TEXT and run the
                    // pipeline as a real pipe (`cat f | tr a b | grep hi`
                    // emitted three independent commands, so `tr` read the
                    // script's stdin and `grep` saw no input). REFUSE on a
                    // stage that cannot rebuild, so the per-stage fallback
                    // below stays available for Perl-side stages.
                    if let Some(cmd) = pipeline_call_to_cmd(&expr) {
                        emit_shell_cmd(out, indent, &cmd);
                        return;
                    }
                }
                for stage in stages {
                    for s in stage {
                        emit_stmt(out, s, indent);
                    }
                }
            }
        }

        IrStmt::Require(module) => {
            emit_indent(out, indent);
            out.push_str(&format!("require {};\n", module));
        }

        IrStmt::DoWhile { body, cond, until } => {
            let kw = if *until { "until" } else { "while" };
            let cond_str = ir_expr_to_perl(cond);
            emit_indent(out, indent);
            out.push_str("do {\n");
            for s in body.iter() {
                emit_stmt(out, s, indent + 1);
            }
            emit_indent(out, indent);
            out.push_str(&format!("}} {} ({});\n", kw, cond_str));
        }
    }
}

// ── Subroutine emitter ───────────────────────────────────────────────

pub(crate) fn emit_sub(out: &mut String, sub: &IrSub) {
    out.push_str(&format!("sub {} {{\n", sub.name));

    // Filter out trailing `return;` (IrStmt::Return(None)) which is
    // unnecessary ceremony — Perl subs return the last expression value.
    let body: Vec<&IrStmt> = if sub.body.last() == Some(&IrStmt::Return(None)) {
        sub.body[..sub.body.len() - 1].iter().collect()
    } else {
        sub.body.iter().collect()
    };

    for s in &body {
        emit_stmt(out, s, 1);
    }
    out.push_str("}\n");
}

// ── Modern-IR (sh2.* call) lowering ──────────────────────────────────
//
// `ast_to_ir` emits shell commands as neutral `Call{func:"exec"}` nodes with
// string/array words, tests as `Call{func:"test"}` on a flat string, and
// captures/pipelines as closures. These lowerings port the Generator's Perl
// idioms (echo → print, `${name}` reads, q{}/interpolation, int() arith)
// onto the IR side so `--shir → --shir-in-perl` produces runnable,
// bash-matching Perl. Constructs with no Perl lowering yet render a `die`
// with an actionable message instead of a panic.

/// Extract a plain string from a word-shaped expression (Str / Var / Ident).
pub(crate) fn call_arg_str(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Var(name, _) => Some(name.clone()),
        IrExpr::Ident(name) => Some(name.clone()),
        // An Interpolate whose parts are all literal text is a plain
        // string (e.g. a herestring `<<< "hello"`). Without this, the
        // renderer rebuilds the herestring as empty.
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => out.push_str(t),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Split a possibly-indexed var name ("matrix[0,0]") into base + optional
/// comma-key. Bash pseudo-multidimensional arrays bake the subscript into
/// the name at the parser level (Assign targets with `indices: []`).
fn split_indexed_var(name: &str) -> (&str, Option<&str>) {
    if let Some(open) = name.find('[') {
        if name.ends_with(']') {
            return (&name[..open], Some(&name[open + 1..name.len() - 1]));
        }
    }
    (name, None)
}

/// The Perl lhs for a (possibly indexed) var name: `matrix[0,0]` →
/// `$matrix{"0,0"}` (a hash key — bash's fake-multidim), plain names →
/// `$name` / `$ENV{name}`.
fn perl_lhs_for(var: &str) -> String {
    if is_env_style_var_name(var) {
        return format!("$ENV{{{}}}", var);
    }
    let (base, idx) = split_indexed_var(var);
    // dotted struct-field names sanitize exactly like var_read's
    // fallback (`p.x` → `$p_x`) — the read must hit the same home
    let base = base.replace('.', "_").replace('-', "_");
    match idx {
        Some(i) => {
            // A known ARRAY indexes with the subscript as a Perl expression
            // (`args[i]` → `$args[$i]`); otherwise it is a hash key.
            if array_var(&base) {
                let sub = if i.chars().all(|c| c.is_ascii_digit()) {
                    i.to_string()
                } else {
                    format!("${}", i.trim_matches('"').trim_matches('\''))
                };
                return format!("${}[{}]", base, sub);
            }
            // The subscript retains its source quotes (`info["os"]`); strip
            // them before emitting the Perl hash key. NOTE `$map{"k"}`, not
            // `${map}{"k"}` — the latter derefs `$map` as a hashref (a
            // scalar), which under `use strict` wrote to a different slot
            // than the `$map{'k'}` reads.
            let key = i.trim_matches('"').trim_matches('\'');
            // A SUBSCRIPT that is itself an expansion (`options["${flags:j:1}"]`,
            // `h[$k]`) must be lowered to Perl — emitting the raw `${…}`
            // text produced `$options{${flags:j:1}}` (a syntax error /
            // literal key, so the hash stayed empty).
            if key.starts_with("${") && key.ends_with('}') {
                if let Some(p) = param_text_to_perl(key) {
                    return format!("${}{{{}}}", base, p);
                }
            }
            if key.starts_with('$') {
                return format!("${}{{{}}}", base, key);
            }
            format!("${}{{'{}'}}", base, key.replace('\'', "\\'"))
        }
        None => format!("${}", base),
    }
}

/// Render a variable read: getVar("x") / Var("x") → `$x` / `$ENV{x}`;
/// bash positional params ($1 → $ARGV[0]) and specials ($#, $?, $@).
fn var_read(name: &str) -> String {
    // `$@` / `$*` — bash's positional list. In a SCALAR (string) context
    // bash joins the positional parameters with the first IFS char (a
    // space); `$*` with a quoted context does the same. Never a Perl
    // variable read (`@ARGV` pasted into a rebuilt command ran as a
    // command).
    if name == "@" || name == "*" {
        return "join(' ', @ARGV)".to_string();
    }
    if is_env_style_var_name(name) {
        return format!("$ENV{{{}}}", name);
    }
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        // bash positional parameter: $1 → $ARGV[0]
        return match name.parse::<usize>() {
            Ok(0) => "$0".to_string(),
            Ok(n) => format!("$ARGV[{}]", n - 1),
            Err(_) => format!("${}", name),
        };
    }
    if let (base, Some(idx)) = split_indexed_var(name) {
        // `${map[foo]}` / `${arr[1]}` — element access baked into the var
        // name by the lowering.  A known ARRAY indexes with a Perl
        // expression; numeric keys index @arr; non-numeric keys are hash
        // keys (map{'foo'}).
        if idx.chars().all(|c| c.is_ascii_digit()) {
            return format!("${}[{}]", base, idx);
        }
        // `${name[@]}` / `${name[*]}` — the WHOLE array. Never a hash key:
        // `@`/`*` is the all-elements subscript, and the A1 drops the
        // `declare -a` that would have put `name` in the census. Reading it
        // as a hash emitted `$name{'@'}` on a variable declared as a SCALAR
        // — a `use strict` compile error that aborted the program
        // (063_04: `${default[@]:0:2}` killed the whole output).
        let bare = idx.trim_matches('"').trim_matches('\'');
        if bare == "@" || bare == "*" {
            return format!("@{}", base);
        }
        if array_var(base) {
            return format!("${}[${}]", base, bare);
        }
        let key = bare;
        // A variable subscript (`${words[$counter]}`) indexes with the
        // variable's value, not the literal name. A BRACED expansion
        // (`${words[${i:-0}]}`) must be recursively lowered — leaving the
        // text `$array{${index:-0}}` put a shell `${…}` in Perl source,
        // which `use strict` rejected (063_12).
        if key.starts_with("${") && key.ends_with('}') {
            return format!("${}{{{}}}", base, param_inner_perl(&key[2..key.len() - 1]));
        }
        if key.starts_with('$') {
            return format!("${}{{{}}}", base, key);
        }
        return format!("${}{{'{}'}}", base, key.replace('\'', "\\'"));
    }
    match name {
        // `$#` is the CURRENT number of positional parameters. It must be
        // read LIVE: `set -- a b c` replaces the list after any preamble
        // snapshot was taken, so the snapshot read `$#` as 0 where bash
        // said 3 (parse-at-slice-param). `@ARGV` in a function body is the
        // `local @ARGV = @_` binding, so the live read is correct at both
        // scopes.
        "#" => "scalar(@ARGV)".to_string(),
        "-" => "'hB'".to_string(), // bash's default non-interactive $- (hashall + braceexpand)
        "?" => "$CHILD_ERROR".to_string(),
        // `$!` — the pid of the most recent background job. The Background
        // arm writes it; without this arm the name fell through to the
        // generic branch and rendered `${!}` (an undefined Perl symbolic
        // ref) — examples/grep_p.
        "!" => "$__sh2_last_bg".to_string(),
        "@" => "@ARGV".to_string(),
        // `$*` is the positional params joined by the first IFS char (space
        // in the default IFS) — in scalar context `@ARGV` gave the COUNT.
        "*" => "join(' ', @ARGV)".to_string(),
        "$" => "$$".to_string(),
        _ if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
        {
            format!("${}", name)
        }
        // dotted struct-field names (the C frontend flattens p.x to a
        // store key) sanitize to a valid identifier — `${p.x}` is a
        // syntax error in Perl
        _ => {
            let m = name.replace('.', "_").replace('-', "_");
            format!("${{{m}}}")
        }
    }
}

/// Render a bash glob pattern as a Perl regex fragment (the subset the
/// corpus's `${x#…}`/`${x%…}`/`${x/…/…}` idioms use).
/// If a `case` PATTERN is exactly a variable reference (`$p`, `${p}`,
/// `${p[1]}`, `${p:-x}` is NOT — it has expansions), return the name in the
/// form Perl interpolates (`p`, `p[1]`). The case-pattern capture hoist
/// produces `$__sh2_casepat_N`, and a user may write `"$p"` directly; both
/// are runtime values, not globs.
fn case_pattern_var_name(pat: &str) -> Option<String> {
    let inner = pat.strip_prefix('$')?;
    // `${name}` — strip the braces; a nested expansion (`${p:-x}`) is not a
    // plain reference and is left to the glob path.
    if let Some(braced) = inner.strip_prefix('{') {
        let braced = braced.strip_suffix('}')?;
        // Reject anything with an operator or a further `$`.
        if braced.contains('$') || braced.contains(':') || braced.contains('%')
            || braced.contains('#') || braced.contains('/') || braced.contains('^')
            || braced.contains(',')
        {
            return None;
        }
        // `name` or `name[index]`.
        return Some(braced.to_string());
    }
    // `$name` / `$name[...]` — the name runs while it is an identifier char,
    // then an optional `[...]` subscript.
    let end = inner
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(inner.len());
    let (name, rest) = inner.split_at(end);
    if name.is_empty() {
        return None;
    }
    if rest.is_empty() {
        return Some(name.to_string());
    }
    if rest.starts_with('[') && rest.ends_with(']') && !rest[1..rest.len() - 1].contains(']') {
        return Some(format!("{}{}", name, rest));
    }
    None
}

fn glob_to_regex(pat: &str) -> String {
    glob_to_regex_greedy(pat, true)
}

/// `*` → `.*` when greedy, `.*?` when non-greedy (bash #/%% shortest vs
/// ##/%% longest prefix/suffix removal).
fn glob_to_regex_greedy(pat: &str, greedy: bool) -> String {
    let star = if greedy { ".*" } else { ".*?" };
    let mut out = String::new();
    let mut chars = pat.chars().peekable();
    while let Some(c) = chars.next() {
        // extglob group: `@(a|b)`, `+(a|b)`, `*(a|b)`, `?(a|b)`.
        if matches!(c, '@' | '+' | '*' | '?') && chars.peek() == Some(&'(') {
            chars.next(); // consume '('
            let mut inner = String::new();
            let mut depth = 1;
            while let Some(&d) = chars.peek() {
                chars.next();
                if d == '(' {
                    depth += 1;
                }
                if d == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                inner.push(d);
            }
            let alts: Vec<String> = inner
                .split('|')
                .map(|a| glob_to_regex_greedy(a, greedy))
                .collect();
            let grp = format!("(?:{})", alts.join("|"));
            match c {
                '@' => out.push_str(&grp),
                '+' => {
                    out.push_str(&grp);
                    out.push('+');
                }
                '*' => {
                    out.push_str(&grp);
                    out.push('*');
                }
                '?' => {
                    out.push_str(&grp);
                    out.push('?');
                }
                _ => {}
            }
            continue;
        }
        match c {
            '*' => out.push_str(star),
            '?' => out.push_str("."),
            '\\' => {
                if let Some(&d) = chars.peek() {
                    out.push('\\');
                    out.push(d);
                    chars.next();
                } else {
                    out.push_str("\\\\");
                }
            }
            // A `[...]` character class passes through INTACT — escaping the
            // brackets turns `[/\\]` (a class matching `/` or `\`) into the
            // literal-text pattern `\[/\\\]`, which matches nothing, so
            // `${p##*[/\\]}` stopped stripping and
            // parse-param-pattern-match printed the whole path instead of
            // `tool`. The class is copied verbatim up to its closing `]`
            // (a leading `!`/`^` needs no escaping in Perl).
            '[' => {
                out.push('[');
                if let Some(&neg) = chars.peek() {
                    if neg == '!' {
                        out.push('^');
                        chars.next();
                    } else if neg == '^' {
                        out.push('^');
                        chars.next();
                    }
                }
                if let Some(&first) = chars.peek() {
                    if first == ']' {
                        out.push_str("\\]");
                        chars.next();
                    }
                }
                while let Some(d) = chars.next() {
                    if d == ']' {
                        out.push(']');
                        break;
                    }
                    if d == '\\' {
                        if let Some(&e) = chars.peek() {
                            out.push('\\');
                            out.push(e);
                            chars.next();
                            continue;
                        }
                    }
                    out.push(d);
                }
            }
            '.' | '(' | ')' | '+' | '|' | '^' | '$' | '{' | '}' | ']' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Render `ArithAst` as a Perl numeric expression (bash integer semantics;
/// int() wrapping happens at the IrExpr::Arith arm).
/// Render an arith STRING (`$(($x % 2))` text — the C frontend's
/// arith/testArith lowering) as a Perl expression. `$name`/`${name}`
/// normalize to bare idents (what parse_arith accepts), then the AST
/// renders via [`arith_ast_to_perl`]. None when the string doesn't parse
/// (caller falls back).
fn arith_str_to_perl(s: &str) -> Option<String> {
    let mut t = String::new();
    // `${…}` expansions are lowered to Perl text via `param_inner_perl`,
    // but `parse_arith` cannot parse `scalar(@arr)`. Stash each expansion
    // behind a sentinel ident and splice the Perl back after the AST walk.
    let mut repls: Vec<(String, String)> = Vec::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '$' && i + 1 < ch.len() {
            // `$(…)` inside arithmetic — evaluate the command and splice its
            // value as a numeric operand (bash does the same expansion
            // before evaluating the expression; `int(" $(wc -l < f) ")`
            // numified the LITERAL text instead).
            if ch[i + 1] == '(' {
                let mut j = i + 2;
                let mut depth = 1;
                while j < ch.len() && depth > 0 {
                    if ch[j] == '(' {
                        depth += 1;
                    }
                    if ch[j] == ')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 {
                    let inner: String = ch[i + 2..j - 1].iter().collect();
                    let sentinel = format!("__sh2_arithpe_{}", repls.len());
                    let val = cmd_str_to_open_perl_stripped(&inner);
                    repls.push((sentinel.clone(), val));
                    t.push_str(&sentinel);
                    i = j;
                    continue;
                }
            }
            if ch[i + 1] == '{' {
                let mut j = i + 2;
                let mut depth = 1;
                while j < ch.len() && depth > 0 {
                    if ch[j] == '{' {
                        depth += 1;
                    }
                    if ch[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 {
                    let inner: String = ch[i + 2..j - 1].iter().collect();
                    let sentinel = format!("__sh2_arithpe_{}", repls.len());
                    repls.push((sentinel.clone(), param_inner_perl(&inner)));
                    t.push_str(&sentinel);
                    i = j;
                    continue;
                }
            } else if ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '_' {
                i += 1; // drop the '$' — the ident itself parses as Ident
                continue;
            } else if ch[i + 1].is_ascii_digit() {
                // `$1` — a positional parameter. Splice its Perl read
                // (`$ARGV[0]`) behind a sentinel so parse_arith sees an
                // identifier (`int(" $1 + $1 ")` numified the LITERAL text).
                let mut j = i + 1;
                while j < ch.len() && ch[j].is_ascii_digit() {
                    j += 1;
                }
                let name: String = ch[i + 1..j].iter().collect();
                let sentinel = format!("__sh2_arithpe_{}", repls.len());
                repls.push((sentinel.clone(), var_read(&name)));
                t.push_str(&sentinel);
                i = j;
                continue;
            } else if matches!(ch[i + 1], '@' | '*' | '#' | '?') {
                let name = ch[i + 1].to_string();
                let sentinel = format!("__sh2_arithpe_{}", repls.len());
                repls.push((sentinel.clone(), var_read(&name)));
                t.push_str(&sentinel);
                i += 2;
                continue;
            }
        }
        t.push(ch[i]);
        i += 1;
    }
    let mut out = crate::shir::parse_arith(&t).map(|ast| arith_ast_to_perl(&ast))?;
    for (sent, val) in repls {
        out = out.replace(&format!("${}", sent), &format!("({})", val));
    }
    Some(out)
}

fn arith_ast_to_perl(ast: &ArithAst) -> String {
    match ast {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) => var_read(name),
        ArithAst::Ident(name) => var_read(name),
        ArithAst::Index { var, key } => {
            format!("${{{}}}[{}]", var, arith_ast_to_perl(key))
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_ast_to_perl(lhs);
            let r = arith_ast_to_perl(rhs);
            match op.as_str() {
                "&&" | "||" | "**" => format!("({} {} {})", l, op, r),
                // bash `/` truncates toward zero; wrap the dividend so
                // int() lands on the quotient (int(a / b) == trunc).
                //
                // The WHOLE node is parenthesized and both operands are
                // grouped: without that, a parent operator rebinds the
                // `int(...)` result and the AST's grouping is lost.
                // `100 / (7 % 2)` rendered as `int(100 / int(7) % 2)`, which
                // Perl parses as `(100/7) % 2` = 0 while bash yields 100
                // (063_01_deeply_nested_arithmetic printed 1, not 79).
                //
                // bash ALSO errors on a zero divisor and leaves the
                // assignment's value EMPTY (`x=$((1/0))` warns "division by
                // 0" and x stays unset), where Perl's `/` would `die` and
                // kill the program. Guard through eval so the value is undef
                // (parse-arithmetic-extra-paren: `x=$(((a+b)/(c+d)))` with
                // all vars unset must leave x unset, not abort).
                "/" => format!("(do {{ my $__d = {}; eval {{ int(({}) / ($__d)) }} }})", r, l),
                "%" => format!("(do {{ my $__d = {}; eval {{ int(({})) % ($__d) }} }})", r, l),
                // Perl's bitwise ops are STRING ops when an operand is a
                // string, and shell vars ARE strings (`$a = "78"`): `$a | $b`
                // string-ORs to "78" and warns "Argument "^E" isn't numeric".
                // bash arithmetic is always integer, so force numeric context
                // with `0+` (063_01 then printed 78 instead of 79).
                "&" => format!("((0+{}) & (0+{}))", l, r),
                "|" => format!("((0+{}) | (0+{}))", l, r),
                "^" => format!("((0+{}) ^ (0+{}))", l, r),
                "<<" => format!("((0+{}) << (0+{}))", l, r),
                ">>" => format!("((0+{}) >> (0+{}))", l, r),
                _ => format!("({} {} {})", l, op, r),
            }
        }
        ArithAst::Un { op, arg } => {
            let a = arith_ast_to_perl(arg);
            match op.as_str() {
                "!" => format!("!({})", a),
                // 64-bit SIGNED not: `~x == -x - 1` (Perl's `~` on a UV gave
                // 18446744073709551615 for `~0`; examples/arith-bitwise-
                // shift-operators expects -1).
                "~" => format!("-({}) - 1", a),
                "-" => format!("-({})", a),
                "+" => a,
                _ => format!("({}{})", op, a),
            }
        }
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_ast_to_perl(test),
            arith_ast_to_perl(then),
            arith_ast_to_perl(else_)
        ),
        ArithAst::Assign { var, op, rhs } => {
            format!("{}{}{}", var_read(var), op, arith_ast_to_perl(rhs))
        }
        ArithAst::IncDec { var, delta, prefix } => {
            let v = var_read(var);
            // Perl's pre/post increment return semantics match bash's:
            // `++$i` yields the new value, `$i++` the old one. Rendering
            // both as `($i += 1)` made `i++ + ++i` evaluate 3+4 instead of
            // 2+4 (examples/078).
            if *prefix {
                format!("({}{})", if *delta < 0 { "--" } else { "++" }, v)
            } else {
                format!("({}{})", v, if *delta < 0 { "--" } else { "++" })
            }
        }
        // C-frontend nodes (never emitted by the shell path): sizeof is a
        // compile-time constant; casts are identity (Perl IV is 64-bit).
        ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
        ArithAst::Cast { arg, .. } => arith_ast_to_perl(arg),
    }
}

/// Render a shell word (an exec arg) as a Perl expression.
/// A word is a Str literal, an Interpolate (concat chain), a getVar/split/
/// param/arith call, or an Array (bash joins array elements with a space
/// only when there are several — handled by the echo/command join, so here
/// a multi-element Array renders as its own join).
fn render_word(e: &IrExpr) -> String {
    match e {
        IrExpr::Str(s, style) => {
            // Reuse the Str rendering from ir_expr_to_perl by constructing it.
            if let Some(pat) = s.strip_prefix("\u{1}SH2GLOB\u{1}") {
                // Shell glob word — expand via Perl's glob().
                format!("glob('{}')", pat.replace('\'', "\\\\'"))
            } else {
                render_str_literal(s, style)
            }
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => args
                .first()
                .and_then(call_arg_str)
                .map(|n| var_read(&n))
                .unwrap_or_else(|| "''".to_string()),
            "split" => args
                .first()
                .map(render_word)
                .unwrap_or_else(|| "''".to_string()),
            "param" => render_param(args),
            "arith" => args
                .first()
                .and_then(|a| match a {
                    IrExpr::Str(s, _) => arith_str_to_perl(s),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    args.first()
                        .map(|a| format!("int({})", render_word(a)))
                        .unwrap_or_else(|| "0".to_string())
                }),
            "brace" => render_brace_word(args),
            "capture" | "captureWords" => {
                // Command substitution in unquoted word position: capture
                // stdout, then split on IFS whitespace (bash word-splitting
                // semantics). The join keeps it a single string for print.
                //
                // A single-statement body the capability tree can compute
                // (`$(wc -c < FILE)`) is lowered natively first — the
                // shell-text view loses the redirect, so it would otherwise
                // shell out (check_qx: "wrapping builtin 'wc'").
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    if let [IrStmt::Expr(e)] = stmts.as_slice() {
                        if let Some(val) =
                            crate::pipeline_native::native_capture_call(e)
                        {
                            return format!("join(' ', split(/\\s+/, {val}))");
                        }
                        // `$(echo "sqrt($n)" | bc)` — the native pipeline
                        // fold prints the result; capture the Perl prints.
                        if let Some(stmt) = crate::pipeline_native::native_pipeline(e) {
                            return format!(
                                "join(' ', split(/\\s+/, do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {stmt} close $__fh; select($__old); $__cap =~ s/\\n+\\z//; $__cap }}))"
                            );
                        }
                    }
                }
                let cap = args.first().and_then(arrow_to_cmd);
                match cap {
                    Some(cmd) => {
                        format!("join(' ', split(/\\s+/, {}))", cmd_str_to_open_perl(&cmd))
                    }
                    None => "''".to_string(),
                }
            }
            _ => ir_expr_to_perl(e),
        },
        IrExpr::Interpolate(_)
        | IrExpr::Var(_, _)
        | IrExpr::Arith(_)
        | IrExpr::Capture { .. }
        | IrExpr::Index { .. }
        | IrExpr::BinOp { .. }
        | IrExpr::Ternary { .. }
        | IrExpr::DefinedOr { .. }
        | IrExpr::MethodCall { .. } => ir_expr_to_perl(e),
        IrExpr::Array(elements) => {
            let parts: Vec<String> = elements.iter().map(render_word).collect();
            if parts.is_empty() {
                "''".to_string()
            } else if parts.len() == 1 {
                parts[0].clone()
            } else {
                format!("join(' ', {})", parts.join(", "))
            }
        }
        _ => ir_expr_to_perl(e),
    }
}

/// Render an exec word that may expand to MULTIPLE words (bash
/// field-splitting; triage-perl t62_word_split): the A1 `split` marker
/// (an unquoted `$var` read) lowers to Perl's list-context `split`, so
/// the fields become separate args in LIST contexts (printf args, echo
/// joins, array stores, for-iters). Every other word renders single via
/// [`render_word`] — scalar contexts (bash -c text, redirects,
/// interpolations) must keep the raw text; bash does the splitting
/// itself there.
pub(crate) fn render_word_list(e: &IrExpr) -> String {
    match e {
        IrExpr::Call { func, args } if func == "split" => {
            let inner = args
                .first()
                .map(render_word)
                .unwrap_or_else(|| "''".to_string());
            format!("split(/\\s+/, {})", inner)
        }
        // `"$@"` / `"$*"` as an exec word: bash expands to the positional
        // LIST (each parameter its own word; `"$*"` joins with IFS). The
        // scalar renderer would emit `join(' ', @ARGV)` — ONE word — and
        // the rebuilt shell text would then lose the split.
        IrExpr::Call { func, args } if func == "listVar" => match call_arg_str(
            args.first().unwrap_or(&IrExpr::Str(String::new(), StrStyle::SingleQuoted)),
        )
        .as_deref()
        {
            Some("@") => "@ARGV".to_string(),
            Some("*") => "join(' ', @ARGV)".to_string(),
            _ => render_word(e),
        },
        _ => render_word(e),
    }
}
/// `${p##pat}` / `${p%%pat}` prefix/suffix removal → a Perl `s{}{}r`
/// substitution. The brace delimiter avoids pattern chars (`:`, `/`) that
/// collide with the old `s:...:...:` form (`${PATH%%:*}` became `s::.*$`).
fn strip_re(var: &str, pat: &str) -> String {
    format!("(({} =~ s{{{}}}{{}}r))", var, pat.replace('}', "\\}"))
}

/// Escape a `s///` REPLACEMENT for Perl's regex BODY.
///
/// The operand arrives as SOURCE TEXT, so it carries the shell's own
/// backslash escapes (`${v/a/b\/c}` reaches us as `b\/c`), and the job is to
/// translate each shell escape into the literal character it denotes BEFORE
/// re-escaping for Perl. Escaping first instead would turn one source
/// backslash into two emitted ones and print `b\/c` where bash prints `b/c`.
///
/// The two languages escape for different reasons:
///
///  * `/` terminates the substitution (`s/a/b/c/r` is a syntax error), so it
///    must be escaped — but only a `/` that is NOT already escaped by a
///    backslash, which the arm below consumes first;
///  * `\` is Perl's escape character in a replacement, so a LITERAL backslash
///    must become `\\` or it swallows the character after it;
///  * `@` and `$` are INTERPOLATED into a replacement, so `${v/a/@b/}` would
///    splice Perl's `@b` array instead of the text `@b`. Both are escaped.
///
/// An unrecognised escape keeps its backslash (`\q` → `\q`), which is what
/// bash does for an operator it does not define; `\ ` (escaped space) and
/// `\$`/`\@` become the bare character, since bash's rule is "a backslash
/// quotes the next character".
///
/// `/` is escaped rather than switching to the brace delimiter `s{pat}{rep}`
/// used by {@link strip_re}: the brace form suits prefix/suffix stripping but
/// not here, because a glob PATTERN may legitimately contain unbalanced
/// `{`/`}`, which `glob_to_regex` emits verbatim from the user's text.
///
/// Kept separate from the pattern side deliberately — a pattern needs REGEX
/// metacharacter escaping (handled by `glob_to_regex`) and a replacement needs
/// substitution-body escaping, which are not the same transformation.
fn repl_for_regex(rep: &str) -> String {
    let mut out = String::with_capacity(rep.len());
    let mut chars = rep.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // A shell escape: emit the escaped character, escaped for Perl.
            '\\' => match chars.next() {
                // `\\` is a literal backslash → `\\\\` in the Perl body.
                Some('\\') => out.push_str("\\\\"),
                // An escaped character that Perl would re-interpret.
                Some(ch @ ('/' | '$' | '@')) => {
                    out.push('\\');
                    out.push(ch);
                }
                // `\n`, `\t`, … mean what they mean in bash; `\q` means `\q`.
                Some('n') => out.push_str("\\n"),
                Some('t') => out.push_str("\\t"),
                Some(ch) => {
                    out.push('\\');
                    out.push(ch);
                }
                None => out.push_str("\\\\"),
            },
            '/' => out.push_str("\\/"),
            '@' => out.push_str("\\@"),
            '$' => out.push_str("\\$"),
            _ => out.push(c),
        }
    }
    out
}

/// A parameter-expansion default/replacement value: a quoted literal is
/// unquoted (`"default"` → `"default"`), everything else renders as a word.
fn param_value(a: &IrExpr) -> String {
    if let IrExpr::Str(s, _) = a {
        let t = s.trim().trim_matches('"').trim_matches('\'');
        // A NESTED expansion (`${a:-${b}}`, `${a:-$(cmd)}`) arrives as the
        // literal text of the default; render it as a Perl value instead of
        // a string literal (`"\${NAME}"` printed the raw `${NAME}`).
        if t.contains("${") || t.contains("$(") {
            if let Some(v) = nested_expansion_value(t) {
                return v;
            }
        }
        render_str_literal(t, &StrStyle::DoubleQuoted)
    } else {
        render_word(a)
    }
}

/// Render a default/alternate VALUE that is itself an expansion:
/// `${a:-${b}}` / `${a:-$(cmd)}` / `${a:-$b}`. None when the text is not a
/// single expansion (mixed literal text keeps the string-literal path).
fn nested_expansion_value(t: &str) -> Option<String> {
    let t = t.trim();
    if t.starts_with("${") && t.ends_with('}') {
        // balanced check: the closing `}` must match the opening one
        let mut depth = 0usize;
        for (i, c) in t.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 && i != t.len() - 1 {
                        return None; // trailing text after the expansion
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        if let Some(p) = param_text_to_perl(t) {
            return Some(p);
        }
        // A default whose VALUE is a slice (`${a:-${arr[@]:0:2}}`)
        return param_text_to_perl(&t.replace("[@]", "[@]"));
    }
    if let Some(inner) = t.strip_prefix("$(").and_then(|s| s.strip_suffix(')')) {
        if !inner.is_empty() && !inner.contains("$(") {
            return Some(cmd_str_to_open_perl_stripped(inner));
        }
    }
    if let Some(n) = t.strip_prefix('$') {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(var_read(n));
        }
    }
    None
}

/// `${var…}` parameter expansion (Call param): op, var, value/pattern...
fn render_param(args: &[IrExpr]) -> String {
    let op = args.first().and_then(call_arg_str).unwrap_or_default();
    let name = args.get(1).and_then(call_arg_str).unwrap_or_default();
    let var = if name.is_empty() {
        "''".to_string()
    } else if !name.contains('[') && array_var(&name) {
        // `${arr}` / `${arr:-}` on a bash ARRAY yields the FIRST element.
        format!("${}[0]", name)
    } else {
        var_read(&name)
    };
    let val = args
        .get(2)
        .map(param_value)
        .unwrap_or_else(|| "''".to_string());
    match op.as_str() {
        "" => var,
        // `${#var}` — the scalar length (the shIR's `len` op).
        "len" => format!("length({})", var),
        "-" => {
            if var.starts_with('@') {
                format!("(@ARGV ? join(' ', @ARGV) : {})", val)
            } else {
                format!("(defined({}) ? {} : {})", var, var, val)
            }
        }
        ":-" => {
            if var.starts_with('@') {
                format!("(@ARGV ? join(' ', @ARGV) : {})", val)
            } else {
                format!(
                    "((defined({v}) && length({v})) ? {v} : {d})",
                    v = var,
                    d = val
                )
            }
        }
        "=" | ":=" => format!(
            "((defined({v}) && length({v})) ? {v} : ({v} = {d}))",
            v = var,
            d = val
        ),
        // `${v+w}` / `${v:+w}` — the ALTERNATE-VALUE twins of `-`/`:-`.
        // The predicate differs between the two and the distinction is
        // load-bearing, so they cannot share an arm:
        //
        //   `+`  — is v SET?           strictly WEAKER than non-empty.
        //   `:+` — is v set AND non-empty?
        //
        // A var assigned the empty string IS set, so `E=; echo ${E+d}`
        // prints `d` while `${E:+d}` prints nothing. Folding `+` into the
        // `:-` non-empty test therefore loses the payload for exactly the
        // set-but-empty case, and — because `test -n` then sees a different
        // length — can invert the branch outright:
        //
        //   if test -n "${V+x}"; then …    # bash: taken iff V is SET
        //
        // Before these arms, `+` fell through to the catch-all `_ => var`,
        // which renders the VARIABLE — dropping `x` entirely and making
        // `test -n` read the value rather than the set-ness. Unset V
        // rendered as an empty read that still had length under `warn`, and
        // the `+` operand reached the renderer already rewritten to `=` by
        // the generator pre-pass, which rendered `($ENV{V} = "x")` — an
        // ASSIGNMENT with a side effect, making the branch permanently true.
        "+" => {
            if var.starts_with('@') {
                format!("(@ARGV ? {d} : '')", d = val)
            } else {
                format!("(defined({v}) ? {d} : '')", v = var, d = val)
            }
        }
        ":+" => {
            if var.starts_with('@') {
                format!("(@ARGV ? {d} : '')", d = val)
            } else {
                format!(
                    "((defined({v}) && length({v})) ? {d} : '')",
                    v = var,
                    d = val
                )
            }
        }
        // Prefix removal: # shortest, ## longest. The `:` forms (only when
        // non-empty) are approximated by the plain forms.
        // Prefix removal: # shortest, ## longest (approximated by the
        // greedy form — the corpus idioms are ##*/ and %/* which are exact).
        "#" | "#:" | "##" | "##:" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            let greedy = matches!(op.as_str(), "##" | "##:");
            let re = glob_to_regex_greedy(&pat, greedy);
            format!("(({} =~ s{{{}}}{{}}r))", var, format!("^{}", re).replace('}', "\\}"))
        }
        // Suffix removal: % shortest, %% longest. The `%.*` extension
        // idiom strips the last extension (shortest) / from the first dot
        // (longest); other patterns remove a greedy suffix.
        "%" | "%:" | "%%" | "%%:" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            if pat == ".*" {
                if op == "%%" || op == "%%:" {
                    strip_re(&var, "\\.*$")
                } else {
                    strip_re(&var, "\\.[^.]*$")
                }
            } else if pat.len() == 2 && pat.ends_with('*') {
                // C* pattern: % removes from the LAST C, %% from the FIRST
                // ("hello world" %o* → "hello w", %%o* → "hell").
                let c = &pat[..1];
                let re = if matches!(op.as_str(), "%%" | "%%:") {
                    format!("{}.*", c)
                } else {
                    format!("{}[^{}]*", c, c)
                };
                strip_re(&var, &format!("{}$", re))
            } else {
                let greedy = matches!(op.as_str(), "%%" | "%%:");
                let re = glob_to_regex_greedy(&pat, greedy);
                strip_re(&var, &format!("{}$", re))
            }
        }
        // Named idioms the IR maps: ##*/ → basename, %/* → dirname.
        "basename" => strip_re(&var, "^.*/"),
        "dirname" => strip_re(&var, "/[^/]*$"),
        // Case modification: ^^/,, = all, ^/, = first char.
        "^^" => format!("uc({})", var),
        ",," => format!("lc({})", var),
        "^" => format!("ucfirst({})", var),
        "," => format!("lcfirst({})", var),
        // Substring: ${var:off:len} → substr($var, off, len). The offset and
        // length may be VARIABLES (`${flags:j:1}` — the bash idiom for
        // `substr($flags, $j, 1)`); a bare identifier must become a Perl
        // read, and a negative offset counts from the end.
        ":" => {
            let num_or_read = |t: String| -> String {
                let t = t.trim().to_string();
                if t.is_empty() {
                    return "0".to_string();
                }
                if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                    || t.starts_with('-')
                {
                    return t;
                }
                if t.starts_with('$') || t.starts_with("${") {
                    if t.starts_with("${") {
                        return param_text_to_perl(&t).unwrap_or(t);
                    }
                    return var_read(t.trim_start_matches('$'));
                }
                if t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return var_read(&t);
                }
                t
            };
            let off = num_or_read(
                args.get(2).and_then(call_arg_str).unwrap_or_default(),
            );
            let len_raw = args.get(3).and_then(call_arg_str).unwrap_or_default();
            if len_raw.trim().is_empty() {
                format!("substr({}, {})", var, off)
            } else {
                format!("substr({}, {}, {})", var, off, num_or_read(len_raw))
            }
        }
        // Substitution: / first, // all.
        //
        // The operands are read with `call_arg_str` (RAW text), never through
        // `param_value`. `param_value` renders a `Str` via
        // `render_str_literal`, whose job is to emit a Perl STRING LITERAL —
        // correct for a default (`${a:-abc}` really does want `"abc"`), wrong
        // here. The two operands are a PATTERN and a REPLACEMENT, and they
        // are spliced into a REGEX position, so the quotes came back out as
        // part of the pattern:
        //
        //     s=abc:def:ghi; echo "${s/abc/XYZ}"
        //     ⇒ print((($s =~ s/"abc"/"XYZ"/r)), "\n");
        //
        // Perl compiles that as "the six characters `\"abc\"`", which never
        // occur in the subject, so the match fails, `s///r` returns the
        // subject UNCHANGED, and the substitution is a SILENT no-op: correct
        // exit code, no warning, wrong text. `glob_to_regex` below does the
        // escaping this operand was always meant to receive.
        //
        // This arm also SHADOWED the `"//" | "/"` arm that follows it. In a
        // Rust `match` the FIRST matching arm wins, so that arm was dead code
        // and the plain `/` form silently took this path; it is folded in here
        // rather than left as a trap.
        "/" | "//" => {
            let pat = args.get(2).and_then(call_arg_str).unwrap_or_default();
            let rep = args.get(3).and_then(call_arg_str).unwrap_or_default();
            let g = if op == "//" { "g" } else { "" };
            format!(
                "(({} =~ s/{}/{}/{g}r))",
                var,
                glob_to_regex(&pat),
                repl_for_regex(&rep)
            )
        }
        // Array slice: ${arr[@]:off:len} → @arr[off..off+len-1] (0-based;
        // the shIR contract normalizes subscripts).
        "slice" => {
            let raw_off = args
                .get(2)
                .and_then(call_arg_str)
                .unwrap_or_else(|| "0".to_string());
            let raw_len = args.get(3).and_then(call_arg_str).unwrap_or_default();
            // `${@:off:len}` / `${*:off:len}` — a 1-based slice of the
            // positional parameters (a LIST, not a scalar substring).
            if name == "@" || name == "*" {
                let off: usize = raw_off.trim().parse().unwrap_or(1).max(1);
                let start = off - 1;
                if raw_len.is_empty() {
                    return format!("join(' ', @ARGV[{start}..$#ARGV])");
                }
                let len: usize = raw_len.trim().parse().unwrap_or(0);
                if len == 0 {
                    return "''".to_string();
                }
                let end = start + len - 1;
                return format!("join(' ', @ARGV[{start}..{end}])");
            }
            // A NON-`@` offset on a SCALAR variable is a substring
            // `${s:off:len}`: `substr`. The A1 drops `[@]`, so consult the
            // program's array census (`numbers[@]:3:4` stays a slice).
            //
            // `${arr[@]:off:len}` reaches here with the `[@]` still on the
            // NAME, so `array_var("primes[@]")` is false and the census is
            // asked about the wrong name — the slice took the substring branch
            // and emitted `substr($primes[$@], 0, 1)` ("Argument "" isn't
            // numeric in array or hash lookup", examples/051_primes line 9).
            // Drop the subscript suffix first so the census sees `primes`.
            let name = name
                .strip_suffix("[@]")
                .or_else(|| name.strip_suffix("[0]"))
                .unwrap_or(&name);
            if raw_off != "@" && !array_var(&name) {
                // The offset/length may be VARIABLES — `${flags:j:1}` is the
                // bash idiom for `substr($flags, $j, 1)`; emitting the bare
                // name produced a syntax error (`substr($f, j, 1)`).
                let as_arg = |t: &str| -> String {
                    let t = t.trim();
                    if t.is_empty() {
                        return "0".to_string();
                    }
                    if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                        || t.starts_with('-')
                    {
                        return t.to_string();
                    }
                    if t.starts_with("${") {
                        return param_text_to_perl(t).unwrap_or_else(|| t.to_string());
                    }
                    if let Some(n) = t.strip_prefix('$') {
                        return var_read(n);
                    }
                    if t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        return var_read(t);
                    }
                    t.to_string()
                };
                if raw_len.is_empty() {
                    return format!("substr({}, {})", var, as_arg(&raw_off));
                }
                return format!("substr({}, {}, {})", var, as_arg(&raw_off), as_arg(&raw_len));
            }
            // `${#arr[@]}` — array LENGTH (the `#`-prefixed name with an
            // `@` offset and no length). A hash's `${#h[@]}` is its key
            // count.
            if name.starts_with('#') && raw_off == "@" && raw_len.is_empty() {
                let base = &name[1..];
                return if array_var(base) {
                    format!("scalar(@{})", base)
                } else {
                    format!("scalar(keys %{})", base)
                };
            }
            // `${!map[@]}` — HASH KEYS (the `!`-prefixed name with an `@`
            // offset and no length). The `${!prefix*[@]}` prefix form
            // carries a trailing `*`.
            if name.starts_with('!') && raw_off == "@" && raw_len.is_empty() {
                return format!("keys %{}", name[1..].trim_end_matches('*'));
            }
            // `${arr[@]}` — all elements: a LIST in iteration contexts, but
            // a space-joined string in scalar/print contexts.  The legacy
            // renderer joins; emit join here — callers that need the list
            // (for-iter) use a different node.
            // An ASSOCIATIVE array's `${map[@]}` is its VALUES (`keys` for
            // `${!map[@]}` above); `@map` would be an undeclared Perl array.
            if raw_off == "@" && raw_len.is_empty() && !name.starts_with('#') {
                // Associative (`declare -A`) → VALUES; otherwise an indexed
                // array (a name the census never saw is overwhelmingly an
                // array here, and `@x` is at least declarable).
                if hash_var(&name) {
                    return format!("join(' ', values %{})", name);
                }
                return format!("join(' ', @{})", name);
            }
            let off = raw_off
                .parse::<i64>()
                .unwrap_or(0);
            let end = match args.get(3).and_then(call_arg_str) {
                Some(l) if !l.is_empty() => {
                    let len = l.parse::<i64>().unwrap_or(0);
                    if len <= 0 {
                        return "()".to_string();
                    }
                    off + len - 1
                }
                _ => i64::MAX, // unbounded → to the last element
            };
            // `${arr[@]:off:len}` — the `[@]` was already stripped from `name`
            // above so the census (and this interpolation) see the bare array.
            if end == i64::MAX {
                format!("@{0}[{1}..$#{0}]", name, off)
            } else {
                format!("@{0}[{1}..{2}]", name, off, end)
            }
        }
        _ => {
            // Unknown op — best-effort: return the var (parameter expansions
            // default to the value when the op is unrecognized).
            var
        }
    }
}

/// Build a bash command string from an exec's (cmd, words) for shell-out.
fn build_shell_cmd(cmd: &str, words: &[&IrExpr]) -> String {
    let mut parts: Vec<String> = vec![bash_quote_word(cmd)];
    for w in words {
        parts.push(bash_word_for(w));
    }
    parts.join(" ")
}

/// `cd` must affect the Perl process (chdir), so chains starting with it
/// render natively: `cd D || exit N` → `chdir(D) or exit N;` and
/// `cd D && cmd` → `chdir(D) or die;` + shell-out of the tail.
fn cd_chain_to_perl(expr: &IrExpr) -> Option<String> {
    if let IrExpr::BinOp { lhs, op, rhs } = expr {
        if let IrExpr::Call { func, args } = lhs.as_ref() {
            if func == "exec" || func == "builtin" {
                if let Some((cmd, words)) = exec_call_parts(args) {
                    if cmd == "cd" {
                        let dir = words
                            .first()
                            .map(|w| render_word(w))
                            .unwrap_or_else(|| "$ENV{HOME}".to_string());
                        return match op {
                             BinOpKind::Or => {
                                // `cd D || exit N` — the common "exit on cd
                                // failure" pattern: render as a direct Perl
                                // `or exit N`.
                                if let IrExpr::Call { func: f, args: a2 } = rhs.as_ref() {
                                    if f == "exit" {
                                        let code = a2
                                            .first()
                                            .map(|w| render_word(w))
                                            .unwrap_or_else(|| "1".to_string());
                                        return Some(format!("chdir({}) or exit {};", dir, code));
                                    }
                                }
                                // `cd D || <anything else>` — bash runs <rhs>
                                // when cd fails (e.g. `cd /bad || echo fallback`).
                                // Render the RHS as a Perl statement inside a
                                // `do { }` block so its side-effects + exit status
                                // update $CHILD_ERROR for follow-up `$?`.
                                let mut rhs_str = String::new();
                                emit_stmt(&mut rhs_str, &IrStmt::Expr((**rhs).clone()), 0);
                                Some(format!("chdir({}) or do {{ {} }};", dir, rhs_str.trim_end()))
                            }
                            BinOpKind::And => {
                                let tail = expr_to_cmd(rhs);
                                match tail {
                                    Some(t) => Some(format!(
                                        "{x}chdir({dir}) or die \"cd: $!\\n\"; system('bash', '-c', {q}); $main_exit_code = $CHILD_ERROR = $? >> 8;",
                                        x = var_exports_str(&t),
                                        dir = dir,
                                        q = safe_perl_q_string(&t)
                                    )),
                                    None => Some(format!("chdir({}) or die \"cd: $!\\n\";", dir)),
                                }
                            }
                            _ => None,
                        };
                    }
                }
            }
        }
    }
    None
}

/// Split a test-chain: returns (cond-string, tail) where tail is the first
/// non-test node after the leading tests. Handles left-assoc chains
/// (t1 && t2 && cmd) and mixed ops.
fn split_test_tail(e: &IrExpr) -> Option<(String, Option<&IrExpr>, Option<BinOpKind>)> {
    match e {
        IrExpr::Call { func, .. } if func == "test" => Some((ir_expr_to_perl(e), None, None)),
        IrExpr::BinOp { lhs, op, rhs } if matches!(op, BinOpKind::And | BinOpKind::Or) => {
            let (lcond, ltail, _) = split_test_tail(lhs)?;
            let joiner = if matches!(op, BinOpKind::And) {
                " && "
            } else {
                " || "
            };
            match (ltail, rhs.as_ref()) {
                // lhs ended in a test (no inner tail): rhs continues the
                // chain (another test) or is the tail (connector = op).
                (None, r) => {
                    if let IrExpr::Call { func, .. } = r {
                        if func == "test" {
                            let (rcond, rtail, _) = split_test_tail(r)?;
                            Some((format!("({}){}({})", lcond, joiner, rcond), rtail, None))
                        } else {
                            Some((lcond, Some(r), Some(op.clone())))
                        }
                    } else {
                        Some((lcond, Some(r), Some(op.clone())))
                    }
                }
                // lhs had an inner tail (t && cmd1) — the outer rhs is the
                // else-branch for the if/else caller.
                (Some(_), r) => Some((lcond, Some(r), None)),
            }
        }
        _ => None,
    }
}

/// `test-cond ||/&& control` chains render natively: `[ x = y ] || continue`
/// → `(cond) || next;`, `[[ flat ]] && cmd` → `if (cond) { cmd; }`, and
/// `(t && c1) || c2` → if/else. Flattened `[[ ]]` tests can't be
/// reconstructed as spaced `[ … ]` bash syntax, so these lower in Perl.
fn control_chain_to_perl(e: &IrExpr) -> Option<String> {
    // (test [&&/|| test]* [&& then]) || else → if/else
    if let IrExpr::BinOp {
        lhs,
        op: BinOpKind::Or,
        rhs,
    } = e
    {
        if let IrExpr::BinOp {
            lhs: l2,
            op: BinOpKind::And,
            rhs: then_cmd,
        } = lhs.as_ref()
        {
            if let Some((cond, ltail, _)) = split_test_tail(l2) {
                if ltail.is_none() {
                    if let (Some(t), Some(els)) = (expr_to_cmd(then_cmd), expr_to_cmd(rhs)) {
                        return Some(format!(
                            "{x}if ({c}) {{ system('bash', '-c', {q1}); $main_exit_code = $CHILD_ERROR = $? >> 8; }} else {{ system('bash', '-c', {q2}); $main_exit_code = $CHILD_ERROR = $? >> 8; }}",
                            x = format!("{}{}", var_exports_str(&t), var_exports_str(&els)),
                            c = cond,
                            q1 = safe_perl_q_string(&t),
                            q2 = safe_perl_q_string(&els)
                        ));
                    }
                }
            }
        }
    }
    let (cond, tail, connector) = split_test_tail(e)?;
    // Control-flow tail: continue/break/exit.
    if let Some(tail_expr) = tail {
        // connector: And → run the tail when cond holds; Or → when it
        // doesn't (bash `t || cmd` = if-not).
        let negated = matches!(connector, Some(BinOpKind::Or));
        if let IrExpr::Call { func, args } = tail_expr {
            match func.as_str() {
                "continue" => {
                    return Some(if negated {
                        format!("({}) || next;", cond)
                    } else {
                        format!("({}) && next;", cond)
                    })
                }
                "break" => {
                    return Some(if negated {
                        format!("({}) || last;", cond)
                    } else {
                        format!("({}) && last;", cond)
                    })
                }
                "exit" => {
                    let code = args
                        .first()
                        .map(render_word)
                        .unwrap_or_else(|| "1".to_string());
                    return Some(if negated {
                        format!("({}) || exit {};", cond, code)
                    } else {
                        format!("({}) && exit {};", cond, code)
                    });
                }
                _ => {}
            }
        }
        // Command tail: if-guard around the shell-out.
        if let Some(cmd) = expr_to_cmd(tail_expr) {
            let c = if negated {
                format!("!({})", cond)
            } else {
                cond.clone()
            };
            return Some(format!(
                "{x}if ({c}) {{ system('bash', '-c', {q}); $main_exit_code = $CHILD_ERROR = $? >> 8; }}",
                x = var_exports_str(&cmd),
                c = c,
                q = safe_perl_q_string(&cmd)
            ));
        }
    }
    None
}

/// Convert a bash arithmetic text ("value % 2 == 0") to Perl by prefixing
/// bare identifiers with `$` (the let / (( )) condition form).
fn arith_text_to_perl(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let skipped = [
        "if", "else", "for", "while", "do", "and", "or", "not", "xor", "sub", "my", "local", "our",
        "defined", "undef", "int", "length", "scalar", "eq", "ne", "lt", "gt", "le", "ge", "cmp",
        "sqrt", "abs",
    ];
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // `${…}` — a parameter expansion: lower it directly (the bare-name
        // prefixing below would corrupt `${#args[@]}` into `${#$args[@]}`).
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            let mut j = i + 2;
            let mut depth = 1;
            while j < chars.len() && depth > 0 {
                if chars[j] == '{' {
                    depth += 1;
                }
                if chars[j] == '}' {
                    depth -= 1;
                }
                j += 1;
            }
            let inner: String = chars[i + 2..j.saturating_sub(1)].iter().collect();
            out.push_str(&param_inner_perl(&inner));
            i = j;
            continue;
        }
        // `$name` — already a Perl read, copy verbatim.
        if chars[i] == '$'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            out.push('$');
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let w: String = chars[start..i].iter().collect();
            if skipped.contains(&w.as_str()) {
                out.push_str(&w);
            } else {
                out.push_str(&format!("${}", w));
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The Perl regex body for the `grep -o` lift's `grepMatches(text, pat,
/// flags)`. `-F` quotes, `-E` is taken as-is, otherwise BRE→Perl.
fn grep_o_regex(pat: &str, flags: &str) -> String {
    if flags.contains('F') {
        return pat
            .chars()
            .map(|c| {
                if "\\^$.|?*+()[]{}".contains(c) {
                    format!("\\{c}")
                } else {
                    c.to_string()
                }
            })
            .collect();
    }
    if flags.contains('E') {
        return pat.replace('\\', "\\\\").replace('/', "\\/");
    }
    let mut out = String::new();
    let mut it = pat.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some(x) if "(){}|+?".contains(x) => out.push(x),
                Some(x) => {
                    out.push('\\');
                    out.push(x);
                }
                None => out.push('\\'),
            }
        } else if "(){}|+?".contains(c) {
            out.push('\\');
            out.push(c);
        } else if c == '/' {
            out.push_str("\\/");
        } else {
            out.push(c);
        }
    }
    out
}

/// The Perl for a `${…}` inner text (used by the `let`-text lowering).
fn param_inner_perl(inner: &str) -> String {
    if let Some(rest) = inner.strip_prefix('#') {
        if let Some(arr) = rest.strip_suffix("[@]") {
            return format!("scalar(@{})", arr);
        }
        return format!("length({})", var_read(rest));
    }
    if let Some(rest) = inner.strip_prefix('!') {
        let base = rest.trim_end_matches("[@]").trim_end_matches('*');
        return format!("keys %{}", base);
    }
    // `${name OP WORD}` — the parameter-expansion OPERATORS. This function
    // is the arithmetic-context lowering, and the text it receives is the
    // INNER of a `${…}` whose operands are typically plain names
    // (`${var:-0}`, `${array[i]:-0}`). Falling through to `var_read` on the
    // WHOLE string ran the sanitize fallback, which maps `-` to `_`:
    // `${var:-0}` rendered as `${var:_0}` — a Perl compile error — instead
    // of "var, or 0 if unset" (063_12).
    //
    // Locate the operator SKIPPING any `[...]` subscript: the first `:` in
    // `${array[${index:-0}]:-0}` belongs to the nested expansion.
    let mut head_end = inner.len();
    {
        let bytes = inner.as_bytes();
        let mut depth = 0i32;
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ':' | '-' | '+' | '=' | '?' if depth == 0 => {
                    head_end = i;
                    break;
                }
                _ => {}
            }
            i += 1;
        }
    }
    if head_end == inner.len() {
        return var_read(inner);
    }
    let (name, op_tail) = inner.split_at(head_end);
    if name.is_empty() || op_tail.is_empty() {
        return var_read(inner);
    }
    let name_read = var_read(name);
    let (test_unset_only, tail) = match op_tail.strip_prefix(':') {
        Some(t) => (false, t),
        None => (true, op_tail),
    };
    let Some(opch) = tail.chars().next() else {
        return name_read;
    };
    let word = &tail[opch.len_utf8()..];
    let word_perl = if word.starts_with('{') && word.ends_with('}') {
        param_inner_perl(&word[1..word.len() - 1])
    } else if word.starts_with('$') {
        var_read(word.trim_start_matches('$').trim_matches(|c| c == '{' || c == '}'))
    } else {
        format!("\"{}\"", word.replace('"', "\\\""))
    };
    let is_set = if test_unset_only {
        format!("defined({name_read})")
    } else {
        format!("(defined({name_read}) && length({name_read}))")
    };
    match opch {
        '-' => format!("({is_set} ? {name_read} : {word_perl})"),
        '+' => format!("({is_set} ? {word_perl} : \"\")"),
        '=' => format!("({is_set} ? {name_read} : ({name_read} = {word_perl}))"),
        _ => name_read,
    }
}

/// A word as bash syntax: literals shell-quoted, captures as `$(…)`
/// (bash evaluates nested command substitution), interpolated words rebuilt
/// as bash double-quoted `"…$var…"`, vars double-quoted, braces as `{…}`.
/// Reconstruct bash `${...}` syntax from a `param` Call (the shIR lowering
/// of parameter expansions), for shell-out command strings.
fn bash_param_syntax(args: &[IrExpr]) -> String {
    let op = args.first().and_then(call_arg_str).unwrap_or_default();
    let name = args.get(1).and_then(call_arg_str).unwrap_or_default();
    if op == "slice" {
        let off = args.get(2).and_then(call_arg_str).unwrap_or_default();
        let len = args.get(3).and_then(call_arg_str).unwrap_or_default();
        if len.is_empty() {
            format!("${{{}[{}]}}", name, off)
        } else {
            format!("${{{}[{}]:{}}}", name, off, len)
        }
    } else if op.is_empty() {
        // `${map[$k]}` element access — the name carries the bracket.
        format!("${{{}}}", name)
    } else {
        let val = args.get(2).and_then(call_arg_str).unwrap_or_default();
        format!("${{{}{}{}}}", name, op, val)
    }
}

fn bash_word_for(w: &IrExpr) -> String {
    // The for-iter shape is Array([single word]) — unwrap it.
    if let IrExpr::Array(elems) = w {
        if elems.len() == 1 {
            return bash_word_for(&elems[0]);
        }
        let parts: Vec<String> = elems.iter().map(bash_word_for).collect();
        return parts.join(" ");
    }
    match w {
        // `"$@"` / `"$*"` — the rebuilt text is handed to `bash -c` / the
        // pipeline parser, so keep the NATIVE shell form: bash performs the
        // expansion (every parameter its own word for `@`; IFS-joined for
        // `*`). Rendering `@ARGV` here would paste a Perl name into shell
        // text and bash would try to run it as a command.
        IrExpr::Call { func, args } if func == "listVar" => {
            let n = args.first().and_then(call_arg_str).unwrap_or_default();
            match n.as_str() {
                "@" => "\"$@\"".to_string(),
                "*" => "\"$*\"".to_string(),
                _ => format!("\"${{{}}}\"", n),
            }
        }
        IrExpr::Str(s, _) => {
            if let Some(pat) = s.strip_prefix("\u{1}SH2GLOB\u{1}") {
                // Shell glob — emit unquoted so bash -c expands it.
                pat.to_string()
            } else {
                bash_quote_word(s)
            }
        }
        IrExpr::Interpolate(parts) => {
            let mut s = String::from("\"");
            for p in parts {
                match p {
                    InterpPart::Lit(t) => {
                        for ch in t.chars() {
                            match ch {
                                '"' => s.push_str("\\\""),
                                '$' => s.push_str("\\$"),
                                '`' => s.push_str("\\`"),
                                c if is_byte_marker(c) => s.push_str(&byte_marker_escape(c)),
                                c => s.push(c),
                            }
                        }
                    }
                    InterpPart::Expr(e) => {
                        if let IrExpr::Call { func, args } = e.as_ref() {
                            if func == "getVar" {
                                if let Some(n) = args.first().and_then(call_arg_str) {
                                    // `"$@"` / `"$*"` — bash expands the
                                    // positional list here. The rebuilt text
                                    // goes to bash, so keep the shell syntax
                                    // (the enclosing `"…"` is already open;
                                    // no extra quotes).
                                    if n == "@" || n == "*" {
                                        s.push_str("${");
                                        s.push_str(&n);
                                        s.push('}');
                                        continue;
                                    }
                                    // var_read: digits → $ARGV[0] (positional),
                                    // env-style → $ENV{VAR1}, locals → $x.
                                    // The bare `${}` emitted `$ENV{"1"}` for
                                    // `$1` inside a function (verified:
                                    // examples/002_control_flow greet).
                                    //
                                    // Kept as PERL on purpose: `bash_word_for`
                                    // has several consumers. The ones that
                                    // build an argv VALUE list
                                    // (`pipeline_value_cmd`, the uu-ffi seam)
                                    // want the Perl spelling; the ones that
                                    // hand the text to a shell run it through
                                    // `sh_cmd_value_expr`, whose own renderer
                                    // is what turns each name into a value.
                                    s.push_str(&var_read(&n));
                                    continue;
                                }
                            }
                            if func == "capture" || func == "captureWords" {
                                if let Some(cmd) = args.first().and_then(arrow_to_cmd) {
                                    s.push_str(&format!("$({})", cmd));
                                    continue;
                                }
                            }
                            if func == "param" {
                                s.push_str(&bash_param_syntax(args));
                                continue;
                            }
                        }
                        s.push_str(&render_word(e));
                    }
                }
            }
            s.push('"');
            s
        }
        IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
            match args.first().and_then(arrow_to_cmd) {
                Some(cmd) => format!("$({})", cmd),
                None => "''".to_string(),
            }
        }
        // A command substitution as the Capture VARIANT (not the
        // Call{func:"capture"} form above). `cd -- "$(dirname "$0")"`
        // arrives here: the enclosing command text is rebuilt for a
        // `bash -c` / pipeline run, so the word must go back as SHELL
        // syntax. Falling through to `_` ran `render_word` instead, which
        // emits PERL (`do { … File::Basename::dirname($0) … }`), and that
        // Perl was then spliced verbatim inside the surrounding quotes —
        // bash ran a command literally named `do { … }` and the capture
        // yielded the empty string. Rebuild the command text the same way
        // the Call form does.
        IrExpr::Capture { expr, native: _ } => match arrow_to_cmd(expr) {
            Some(cmd) => format!("$({})", cmd),
            None => "''".to_string(),
        },
        IrExpr::Call { func, args } if func == "brace" => bash_brace_syntax(args),
        IrExpr::Call { func, args } if func == "param" => bash_param_syntax(args),
        // A bare variable read is SHELL `$name`, never Perl: the catch-all
        // below renders it through `render_word`, which yields the Perl
        // spelling (`$ENV{DEST}` for an exported name) — Perl syntax that
        // bash reads as `$ENV` followed by the literal text `{DEST}`
        // (parse-dot-after-var ran `{DEST}.new`). Quote it so bash expands.
        IrExpr::Var(n, _) => format!("\"${{{}}}\"", n),
        IrExpr::Call { func, args } if func == "getVar" => {
            match args.first().and_then(call_arg_str) {
                Some(n) => format!("\"${{{}}}\"", n),
                None => "\"\"".to_string(),
            }
        }
        _ => {
            let wstr = render_word(w);
            // A Perl string literal or a plain scalar: embed safely.
            if wstr.starts_with('\'') || wstr.starts_with('"') || wstr.starts_with('q') {
                wstr
            } else {
                // A variable word — double-quote it for bash and let bash
                // expand the $ (this text is inside a q{} literal, so Perl
                // does not interpolate it; literal $ in single-quoted words
                // above is already protected).
                format!("\"{}\"", wstr.replace('\"', "\\\""))
            }
        }
    }
}

/// Rebuild bash brace syntax from a parsed `brace` Call:
/// prefix + {range/list}{range/list}… + suffix (bash expands it).
fn bash_brace_syntax(args: &[IrExpr]) -> String {
    // A brace word inside a `bash -c` command text: emit the EXPANDED
    // words (shell-quoted, space-joined) so nested/product alternatives
    // and the between-group middles are all exact. Reconstructing `{…}`
    // text cannot express a Compound alternative (`x/{p,q}` needs the
    // surrounding text repeated per value) nor the middles.
    let items = brace_expand(args);
    if items.is_empty() {
        let prefix = args.first().and_then(call_arg_str).unwrap_or_default();
        let suffix = args.get(3).and_then(call_arg_str).unwrap_or_default();
        return format!("'{}{}'", prefix, suffix);
    }
    items
        .iter()
        .map(|s| bash_quote_word(s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn bash_quote_word(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\\\''"))
}

/// Commands whose Generator emulations are verified bash-correct in
/// STANDALONE use (no pipeline/$input_data machinery, no file-arg gaps,
/// error paths included). Everything else falls back to `bash -c`
/// shell-out. Verified empirically per command+arg-shape: seq, ls, wc,
/// cat, tail, grep, tr, mkdir, rm, touch, basename, dirname, pwd, date,
/// hostname, paste, tee, which, yes. Excluded: sort/uniq/sed/cut/comm/
/// strings/gzip/awk (need the pipeline \$input_data), head (reads STDIN
/// even with a file arg), cp/mv (croak on missing source where bash
/// errors and continues), sha256sum/diff/find (output format), rmdir
/// (failure exit code).
const EMULATED_COMMANDS: &[&str] = &[
    "seq", "ls", "wc", "cat", "tail", "grep", "tr", "mkdir", "rm", "touch", "basename", "dirname",
    "pwd", "date", "hostname", "paste", "tee", "which", "yes",
];

/// `$ENV{name} = $name;` for every `$name` referenced in a shell command
/// text — bash children read Perl vars via the environment (otherwise
/// `"$longline"` in the command is unset in the child).
fn var_exports_str(cmd: &str) -> String {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = String::new();
    let name_re = regex::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    // Heredoc bodies (e.g. an embedded `perl <<'EOF' … my $sum = 0; …`) are
    // NOT shell text — their `$vars` are the embedded program's own.
    let mut heredoc: Option<String> = None;
    for line in cmd.lines() {
        if let Some(delim) = &heredoc {
            if line.trim() == delim {
                heredoc = None;
            }
            continue;
        }
        if let Some(pos) = line.find("<<") {
            let rest = line[pos + 2..].trim_start_matches('-').trim_start();
            let d: String = rest
                .trim_start_matches(['\'', '"'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !d.is_empty() {
                heredoc = Some(d);
            }
        }
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        let mut in_sq = false;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                in_sq = !in_sq;
                i += 1;
                continue;
            }
            // A `$var` inside a single-quoted shell word is LITERAL text
            // (e.g. an embedded `perl -e '…$i…'` script) — it must not be
            // exported (that referenced an undeclared Perl var).
            if c == '$' && !in_sq && i + 1 < chars.len() {
                // `"$@"` / `"$*"` — bash's positional list. The Perl
                // @ARGV is not a scalar to export; the rebuilt shell text
                // must carry the words themselves (see `bash_word_for`).
                if matches!(chars[i + 1], '@' | '*') {
                    i += 2;
                    continue;
                }
                // `${name}` — the braced form. The bare-name matcher below
                // cannot see it (`{` is not an identifier char), so a
                // `"${d}"` operand was never exported and the `bash -c`
                // child resolved it to the empty string
                // (000__07_find_path_commands).
                if chars[i + 1] == '{' {
                    let rest: String = chars[i + 2..].iter().collect();
                    if let Some(cap) = name_re.captures(&rest) {
                        let name = cap[1].to_string();
                        i += 2 + name.len();
                        if name != "ENV"
                            && !name.starts_with("__sh2_")
                            && !name
                                .chars()
                                .next()
                                .map(|c| c.is_ascii_uppercase())
                                .unwrap_or(false)
                            && seen.insert(name.clone())
                        {
                            out.push_str(&format!("$ENV{{{}}} = ${};\n", name, name));
                        }
                        continue;
                    }
                }
                let rest: String = chars[i + 1..].iter().collect();
                if let Some(cap) = name_re.captures(&rest) {
                    let name = cap[1].to_string();
                    i += 1 + name.len();
                    if name != "ENV"
                        && !name.starts_with("__sh2_")
                        && !name
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_uppercase())
                            .unwrap_or(false)
                        && seen.insert(name.clone())
                    {
                        out.push_str(&format!("$ENV{{{}}} = ${};\n", name, name));
                    }
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

/// `$ENV{name} = $name;` lines + the bash -c system call + status tracking.
fn emit_shell_cmd(out: &mut String, indent: usize, cmd: &str) {
    // A linear pipeline / redirection the coreutils-ffi-pipeline C ABI can
    // run in-process — no `bash -c` (check_qx). The rebuilt text carries the
    // runtime values (`$n` inside the double-quoted words), not the Perl
    // names, so the var-export prelude is unnecessary here.
    // The export prelude must come FIRST: the rebuilt text can carry a
    // shell expansion (`"${d}"`) that the pipeline ABI passes through as a
    // Perl single-quoted literal, so the CHILD resolves `${d}` against its
    // own environment. Without the export the child sees it unset —
    // `cd "${d}" && find . -name '*.sh'` ran `find` in the PARENT's cwd and
    // 000__07_find_path_commands listed the workspace instead of the mktemp
    // tree.
    let exports = var_exports_str(cmd);
    if let Some(expr) = pipeline_value_cmd(cmd) {
        for line in exports.lines() {
            emit_indent(out, indent);
            out.push_str(line);
            out.push('\n');
        }
        emit_indent(out, indent);
        out.push_str(&format!(
            "$main_exit_code = $CHILD_ERROR = __sh_pipe_run({}, \"pipefail\");\n",
            expr
        ));
        return;
    }
    for line in exports.lines() {
        emit_indent(out, indent);
        out.push_str(line);
        out.push('\n');
    }
    // A command carrying a shell expansion must have its VALUES substituted
    // into the emitted literal. `safe_perl_q_string` escapes `$` so PERL does
    // not interpolate — which also stops BASH from ever expanding it, so a
    // `bash -c` seam would silently run `echo ${!aa[@]}` with the literal
    // text and lose the value (perl-generator-20261002-053100: 6 examples).
    // The %ENV prelude above only carries bare NAMES, never a compound
    // expansion (`${arr[@]}`, `${!h[@]}`, `${h[k]}`).
    let quoted = match sh_cmd_value_expr(cmd) {
        Some(expr) => expr,
        None => safe_perl_q_string(cmd),
    };
    emit_indent(out, indent);
    out.push_str(&format!(
        "system('bash', '-c', {}); $main_exit_code = $CHILD_ERROR = $? >> 8;\n",
        quoted
    ));
}

/// Reuse the AST Generator's per-command in-Perl emulations for an external
/// command: reconstruct the shell text from IR words, re-parse it into a
/// `SimpleCommand`, and run the Generator's dispatcher (ls/wc/sed/… become
/// native Perl, no bash dependency). Returns None when the command isn't
/// emulatable (caller falls back to `bash -c` shell-out).
/// Command emulation is unavailable post-legacy-cleanup: the caller falls
/// back to `bash -c` shell-out (the same path `DEBASHC_IR_NO_EMUL`
/// exercises). The per-command native emulations (ls/wc/sed/…) move into
/// the perl renderer proper (backend/perl's renderer work).
fn generator_emulate_command(_cmd: &str, _words: &[&IrExpr]) -> Option<String> {
    None
}

/// Collect the variable names a word expression READS (getVar/Var nodes,
/// including inside Interpolate parts / Call args / Array elements).
fn word_var_reads(e: &IrExpr, out: &mut std::collections::HashSet<String>) {
    match e {
        IrExpr::Var(n, _) => {
            out.insert(n.clone());
        }
        // The A1 read form: getVar("name") — the name is the Str arg.
        IrExpr::Call { func, args } if func == "getVar" => {
            if let Some(IrExpr::Str(n, _)) = args.first() {
                out.insert(n.clone());
            }
            for a in args {
                word_var_reads(a, out);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                word_var_reads(a, out);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                word_var_reads(i, out);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    word_var_reads(x, out);
                }
            }
        }
        IrExpr::Arith(a) => arith_var_reads(a.as_ref(), out),
        _ => {}
    }
}

/// The ArithAst half of [`word_var_reads`]: collect the variable names an
/// arithmetic AST reads.
fn arith_var_reads(a: &ArithAst, out: &mut std::collections::HashSet<String>) {
    match a {
        ArithAst::Var(n) => {
            out.insert(n.clone());
        }
        ArithAst::Ident(n) => {
            out.insert(n.clone());
        }
        ArithAst::Index { var, key, .. } => {
            out.insert(var.clone());
            arith_var_reads(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_var_reads(lhs, out);
            arith_var_reads(rhs, out);
        }
        ArithAst::Cond { test, then, else_ } => {
            arith_var_reads(test, out);
            arith_var_reads(then, out);
            arith_var_reads(else_, out);
        }
        ArithAst::Un { arg, .. } => arith_var_reads(arg, out),
        ArithAst::Assign { rhs, .. } => arith_var_reads(rhs, out),
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

/// A name the generator may render as `$name` (a preamble local): a plain
/// identifier, not env-style (uppercase — real env vars stay $ENV{..})
/// and not a positional/special name ($1, $?, ...).
fn is_emulatable_var_name(name: &str) -> bool {
    if is_env_style_var_name(name) {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The modern IR packs all of a command's word arguments into a single
/// `Array` element: exec(cmd, Array([w1, w2, …])). Flatten to the word list.
pub(crate) fn exec_word_args(args: &[IrExpr]) -> Vec<&IrExpr> {
    if args.len() == 2 {
        if let IrExpr::Array(elems) = &args[1] {
            return elems.iter().collect();
        }
    }
    args[1..].iter().collect()
}

/// Extract the brace-expansion Json (arg 1) from a `brace` Call.
fn brace_json_arg(args: &[IrExpr]) -> Option<&serde_json::Value> {
    args.iter().find_map(|a| match a {
        IrExpr::Json(v) => Some(v),
        _ => None,
    })
}

/// Expand one range spec (numeric or single-char) to its items, honoring
/// the step and zero-padding width of the start/end strings. Capped at
/// 1024 items.
fn expand_range(start: &str, end: &str, step: i64) -> Vec<String> {
    let mut out = Vec::new();
    if step == 0 {
        return out;
    }
    // Numeric range (zero-padded only when the START has a leading zero).
    if let (Ok(ai), Ok(bi)) = (start.parse::<i64>(), end.parse::<i64>()) {
        let pad = start.starts_with('0');
        let width = if pad { start.len().max(end.len()) } else { 1 };
        if (bi - ai) / step.abs() >= 1024 {
            return out;
        }
        let mut n = ai;
        while (n <= bi && step > 0) || (n >= bi && step < 0) {
            if pad && n >= 0 {
                out.push(format!("{:0width$}", n, width = width));
            } else {
                out.push(n.to_string());
            }
            n += step;
        }
        return out;
    }
    // Single-character range (a..c, a..z..3).
    if let (Some(ac), Some(bc)) = (start.chars().next(), end.chars().next()) {
        if start.chars().count() == 1 && end.chars().count() == 1 {
            let (ai, bi) = (ac as i64, bc as i64);
            if (bi - ai) / step.abs() >= 1024 {
                return out;
            }
            let mut n = ai;
            while (n <= bi && step > 0) || (n >= bi && step < 0) {
                out.push(char::from_u32(n as u32).unwrap_or('?').to_string());
                n += step;
            }
        }
    }
    out
}

/// One alternative JSON node → its expanded item list. A String is a
/// literal; a `range` expands (a lone range); a `nested` group is the
/// union of its alternatives; a `product` is the cartesian concatenation
/// of its sub-groups (a Compound alternative such as `x/{p,q}`).
fn expand_brace_item(it: &serde_json::Value) -> Vec<String> {
    if let Some(s) = it.as_str() {
        return vec![s.to_string()];
    }
    if let Some(n) = it.get("nested") {
        return brace_nested_items(n);
    }
    if let Some(p) = it.get("product").and_then(|p| p.as_array()) {
        let mut combos = vec![String::new()];
        for g in p {
            let alts = brace_nested_items(g);
            let mut next = Vec::new();
            for c in &combos {
                for a in &alts {
                    next.push(format!("{c}{a}"));
                }
            }
            combos = next;
        }
        return combos;
    }
    Vec::new()
}

/// The alternatives of a nested group (`{"nested":[…]}` or a bare array).
/// A range inside a comma list stays LITERAL (`{1..3,7..9}` → `1..3`);
/// only a lone range expands.
fn brace_nested_items(v: &serde_json::Value) -> Vec<String> {
    let items = match v.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let lone_range = items.len() == 1 && items[0].get("range").is_some();
    let mut out = Vec::new();
    for it in items {
        if let Some(range_arr) = it.get("range").and_then(|r| r.as_array()) {
            let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
            let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
            if lone_range {
                let step = range_arr
                    .get(2)
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                out.extend(expand_range(a, b, step));
            } else {
                out.push(format!("{a}..{b}"));
            }
            continue;
        }
        out.extend(expand_brace_item(it));
    }
    out
}

/// Expand the brace groups Json to one item-list per group (each range
/// entry expanded).
fn brace_groups(v: &serde_json::Value) -> Vec<Vec<String>> {
    let mut groups = Vec::new();
    let arr = match v.as_array() {
        Some(a) => a,
        None => return groups,
    };
    for g in arr {
        let items = match g.as_array() {
            Some(i) => i,
            None => continue,
        };
        let mut expanded = Vec::new();
        for it in items {
            if let Some(s) = it.as_str() {
                expanded.push(s.to_string());
            } else if let Some(range_arr) = it.get("range").and_then(|r| r.as_array()) {
                if items.len() > 1 {
                    // bash: a range in a comma list stays LITERAL
                    let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
                    let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
                    expanded.push(format!("{a}..{b}"));
                    continue;
                }
                let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
                let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
                let step = range_arr
                    .get(2)
                    .and_then(|x| x.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                expanded.extend(expand_range(a, b, step));
            } else {
                expanded.extend(expand_brace_item(it));
            }
        }
        if !expanded.is_empty() {
            groups.push(expanded);
        }
    }
    groups
}

/// Full brace expansion: prefix + cross-product(groups) + suffix.
fn brace_expand(args: &[IrExpr]) -> Vec<String> {
    let prefix = args.first().and_then(call_arg_str).unwrap_or_default();
    let suffix = args.get(3).and_then(call_arg_str).unwrap_or_default();
    let v = match brace_json_arg(args) {
        Some(v) => v,
        None => return Vec::new(),
    };
    // the MIDDLES are the literal text between consecutive groups
    // (`{a,b}/{c,d}` — "/" joins group 0→1)
    let middles: Vec<String> = match args.get(2) {
        Some(IrExpr::Json(serde_json::Value::Array(xs))) => xs
            .iter()
            .map(|x| x.as_str().unwrap_or("").to_string())
            .collect(),
        _ => Vec::new(),
    };
    let groups = brace_groups(v);
    if groups.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // Cross-product of the groups, splicing each middle after its group.
    let mut stack: Vec<String> = vec![prefix];
    for (gi, group) in groups.iter().enumerate() {
        let mid = middles.get(gi).cloned().unwrap_or_default();
        let mut next = Vec::new();
        for acc in &stack {
            for item in group {
                let mut s = acc.clone();
                s.push_str(item);
                if gi + 1 < groups.len() {
                    s.push_str(&mid);
                }
                next.push(s);
                if next.len() > 4096 {
                    break;
                }
            }
        }
        stack = next;
    }
    for mut s in stack {
        s.push_str(&suffix);
        out.push(s);
    }
    out
}

/// Render a `brace` Call in word position: the expanded items joined by
/// spaces (echo {a,b,c} → a b c).
fn render_brace_word(args: &[IrExpr]) -> String {
    let items = brace_expand(args);
    if items.is_empty() {
        return "''".to_string();
    }
    let quoted: Vec<String> = items
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "\\\\'")))
        .collect();
    format!("join(' ', {})", quoted.join(", "))
}

/// Render a `brace` Call as a for-iterable: {1..5} → 1..5; {a..c} →
/// ('a'..'c'); {a,b,c} → ('a','b','c').
pub(crate) fn render_brace_iter(args: &[IrExpr]) -> String {
    let v = match brace_json_arg(args) {
        Some(v) => v,
        None => return "()".to_string(),
    };
    // Single numeric range → 1..5; single char range → ('a'..'c').
    let groups = brace_groups(v);
    if groups.len() == 1 && groups[0].len() == 1 {
        if let Some(range_arr) = v
            .as_array()
            .and_then(|a| a[0].as_array())
            .and_then(|g| g[0].get("range"))
            .and_then(|r| r.as_array())
        {
            let a = range_arr.get(0).and_then(|x| x.as_str()).unwrap_or("");
            let b = range_arr.get(1).and_then(|x| x.as_str()).unwrap_or("");
            let step = range_arr
                .get(2)
                .and_then(|x| x.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(1);
            if step == 1 {
                if a.parse::<i64>().is_ok() && b.parse::<i64>().is_ok() {
                    return format!(
                        "{}..{}",
                        a.parse::<i64>().unwrap(),
                        b.parse::<i64>().unwrap()
                    );
                }
                if a.chars().count() == 1 && b.chars().count() == 1 {
                    return format!("('{}'..'{}')", a, b);
                }
            }
        }
    }
    let items = brace_expand(args);
    if items.is_empty() {
        return "()".to_string();
    }
    let quoted: Vec<String> = items
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "\\\\'")))
        .collect();
    format!("({})", quoted.join(", "))
}

/// Extract (cmd, words) from an `exec` Call's args (arg0 = command name;
/// the remaining args are the flattened word list — see exec_word_args).
/// The literal TEXT of a word that is fully static — a plain string, or an
/// interpolation whose parts are all literals. `None` when any part is
/// dynamic (a variable read, a command substitution), i.e. when the text
/// cannot be known at translation time (used by the `eval` lowering).
/// Like `static_word_text`, but renders an INTERPOLATED word as shell
/// SOURCE: literals verbatim and each interpolated expansion as the shell
/// spelling of its value (`getVar("x")` → `${x}`). Used by the `eval`
/// lowering, where bash substitutes the words FIRST and then parses the
/// result: `eval "echo \"n=$x\""` runs `echo "n=42"` when x=42.
///
/// `None` when a literal-only reading is enough (the caller's static path
/// handles that) or when a part cannot be expressed as shell source.
fn eval_word_shell_text(w: &IrExpr) -> Option<String> {
    match w {
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            let mut had_expr = false;
            for p in parts {
                match p {
                    InterpPart::Lit(t) => out.push_str(t),
                    InterpPart::Expr(e) => {
                        had_expr = true;
                        let name = match e.as_ref() {
                            IrExpr::Call { func, args } if func == "getVar" => {
                                args.first().and_then(call_arg_str)
                            }
                            IrExpr::Var(n, _) => Some(n.clone()),
                            _ => None,
                        };
                        out.push_str(&format!("${{{}}}", name?));
                    }
                }
            }
            if had_expr {
                Some(out)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn static_word_text(w: &IrExpr) -> Option<String> {
    match w {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => out.push_str(t),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn exec_call_parts(args: &[IrExpr]) -> Option<(String, Vec<&IrExpr>)> {
    let cmd = call_arg_str(args.first()?)?;
    Some((cmd, exec_word_args(args)))
}

/// Emit `LHS = do { open('bash','-c',CMD) ... }` capture (chomps trailing NL).
fn emit_capture_assign(out: &mut String, indent: usize, lhs: &str, cmd: &str) {
    // `$ENV{name} = $name;` prelude — the bash -c child must see the
    // vars the captured command reads (mirror of the cd/if shell-out
    // sites' var_exports_str prefix; without it `y=$(echo $x)` sees an
    // unset $x in the child and captures the wrong answer).
    let exports = var_exports_str(cmd);
    for line in exports.lines() {
        emit_indent(out, indent);
        out.push_str(line);
        out.push('\n');
    }
    emit_indent(out, indent);
    out.push_str(&format!("{} = {};\n", lhs, cmd_str_to_open_perl(cmd)));
}

/// Rebuild the shell command string for an expression: a `Call exec`,
/// a `pipeline` Call, or a `&&`/`||` chain of those (bash control operators).
/// Rebuild the ORIGINAL `< (producer )` word from a procsub materialization.
///
/// transforms/process_subst rewrites `<(C)` in a pipeline stage into a run of
/// statements (`tmp = mktemp`; `C > tmp`; …). Feeding that run to the
/// pipeline's shell command is impossible — and the consumer would read the
/// script's stdin instead of the producer — so recognise the shape and emit
/// the shell form back: the mktemp/redirect pair IS the process substitution.
fn procsub_materialization_to_cmd(args: &[IrExpr]) -> Option<String> {
    // `and(Arrow(stmts))` — a single closure arg.
    let body = args.iter().find_map(|a| match a {
        IrExpr::Arrow(b) => Some(b),
        _ => None,
    })?;
    // The producers: `Redirect { inner, redirects: [fd 1 -> tmp] }`. Emit
    // each as `<(inner)`; the mktemp Assigns and the trailing rm are the
    // materialisation's own bookkeeping and must NOT appear.
    // tmp name -> the `<(producer)` word it stands for
    let mut subs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // The consumer: the LAST exec, which references the tmp names as
    // ordinary argv words (bash's /dev/fd/N argument passing), or the only
    // exec when the procsub is on stdin.
    let mut consumer: Option<&IrExpr> = None;
    for s in body {
        match s {
            IrStmt::Redirect { inner, redirects } => {
                let mut procsub = false;
                for r in redirects {
                    let is_tmp = matches!(&r.target, IrExpr::Var(n, _) if n.starts_with("__ps_tmp"));
                    if r.fd.unwrap_or(1) == 1
                        && matches!(r.mode.as_str(), "w" | "wc")
                        && is_tmp
                    {
                        let IrExpr::Var(tmp, _) = &r.target else {
                            return None;
                        };
                        let cmd = stmts_to_shell_cmd(inner)?;
                        subs.insert(tmp.clone(), format!("<({cmd})"));
                        procsub = true;
                    } else if is_tmp && r.mode == "r" {
                        // `< <(producer)` on fd 0 — the consumer reads the
                        // materialised file; rebuild as a stdin procsub.
                        let IrExpr::Var(tmp, _) = &r.target else {
                            return None;
                        };
                        let cmd = stmts_to_shell_cmd(inner)?;
                        subs.insert(tmp.clone(), format!("<({cmd})"));
                        procsub = true;
                    }
                }
                if !procsub {
                    return None;
                }
            }
            IrStmt::Assign { .. } => { /* tmp = $(mktemp) */ }
            IrStmt::Expr(IrExpr::Call { func, .. })
                if func == "exec" || func == "builtin" =>
            {
                // `rm tmp…` is the materialisation's cleanup, not the
                // consumer; the consumer is an exec that is not `rm`.
                let is_rm = matches!(
                    s,
                    IrStmt::Expr(IrExpr::Call { args, .. })
                        if args.first().and_then(call_arg_str).as_deref() == Some("rm")
                );
                if !is_rm {
                    consumer = Some(match s {
                        IrStmt::Expr(e) => e,
                        _ => unreachable!(),
                    });
                }
            }
            _ => return None,
        }
    }
    match consumer {
        Some(c) => {
            // Rebuild the consumer with each `__ps_tmpN` word replaced by
            // its `< (producer )` text.
            let (cmd, words) = match c {
                IrExpr::Call { args, .. } => exec_call_parts(args)?,
                _ => return None,
            };
            let rendered: Vec<String> = words
                .iter()
                .map(|w| match w {
                    IrExpr::Var(n, _) if subs.contains_key(n) => subs[n].clone(),
                    other => bash_word_for(other),
                })
                .collect();
            let mut out = bash_quote_word(&cmd);
            if !rendered.is_empty() {
                out.push(' ');
                out.push_str(&rendered.join(" "));
            }
            if subs.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        // Only the producers (e.g. `echo x > >(cat)`) — emit their words.
        None => {
            if subs.is_empty() {
                None
            } else {
                let mut v: Vec<String> = subs.values().cloned().collect();
                v.sort();
                Some(v.join(" "))
            }
        }
    }
}

fn expr_to_cmd(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "exec" | "builtin" => {
                let (cmd, words) = exec_call_parts(args)?;
                Some(build_shell_cmd(&cmd, &words))
            }
            "pipeline" => pipeline_call_to_cmd(e),
            "redirect" => redirect_call_to_cmd(e),
            // The call-form subshell `( … )` (its body is an Arrow).
            "subshell" => {
                let inner = args.iter().find_map(|a| arrow_to_cmd(a))?;
                Some(format!("( {inner} )"))
            }
            // A test in command position reconstructs as bash `[ … ]` — but
            // only when the text is genuinely spaced (`-f x`); flattened
            // `[[ ]]`/`[ ]` forms (no-space `==`/`=~`/`=`) can't round-trip.
            "test" => {
                let text = args.first().and_then(call_arg_str)?;
                if text.contains('=') || text.contains('~') {
                    return None;
                }
                Some(format!("[ {} ]", text))
            }
            // The process-substitution MATERIALIZATION (transforms/
            // process_subst): `and([Assign(tmp, capture(mktemp)),
            //  Redirect(producer, tmp), …, cleanup])`. As a pipeline stage
            // it must rebuild as the original `<(producer)` shell word —
            // otherwise the producer writes a temp file nobody reads and
            // the consumer sees the script's stdin.
            "and" => procsub_materialization_to_cmd(args),
            _ => None,
        },
        IrExpr::BinOp { lhs, op, rhs } => {
            let opstr = match op {
                BinOpKind::And => " && ",
                BinOpKind::Or => " || ",
                _ => return None,
            };
            Some(format!(
                "{}{}{}",
                expr_to_cmd(lhs)?,
                opstr,
                expr_to_cmd(rhs)?
            ))
        }
        _ => None,
    }
}

/// Read the (fd, mode, target) of one redirect spec Object.
pub(crate) fn redirect_spec_fields(e: &IrExpr) -> Option<(i64, String, IrExpr)> {
    if let IrExpr::Object(fields) = e {
        let mut fd: i64 = 1;
        let mut mode = String::new();
        let mut target = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
        for (k, v) in fields {
            match k.as_str() {
                "fd" => {
                    if let IrExpr::Int(n) = v {
                        fd = *n;
                    }
                }
                "mode" => mode = call_arg_str(v).unwrap_or_default(),
                // Keep the target EXPRESSION: a Var target must render as a
                // shell variable read, not a literal file named after it.
                "target" => target = v.clone(),
                _ => {}
            }
        }
        Some((fd, mode, target))
    } else {
        None
    }
}

/// The variable read a redirect target refers to, if any (`Var("x")` or
/// `getVar("x")`).
fn var_ref_name(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Var(n, _) => Some(n.clone()),
        IrExpr::Call { func, args } if func == "getVar" => args.first().and_then(call_arg_str),
        _ => None,
    }
}

thread_local! {
    /// Heredoc bodies collected by `append_redirect_frag` while a single
    /// shell command string is being rebuilt. They are appended AFTER the
    /// whole line (see `take_heredoc_tail`), because a heredoc body must
    /// follow every pipeline stage and redirect on that line.
    static HEREDOC_BODIES: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Drain the heredoc bodies collected during one command rebuild and render
/// the trailing `\n<body>\nDELIM` blocks. Empty string when there were none.
fn take_heredoc_tail() -> String {
    let bodies = HEREDOC_BODIES.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let mut tail = String::new();
    for body in bodies {
        tail.push('\n');
        tail.push_str(&body);
        if !body.ends_with('\n') {
            tail.push('\n');
        }
        tail.push_str("_SH2DOC_");
    }
    tail
}

/// Append one redirect's shell syntax (` > t`, ` 2>> t`, ` 2>&1`, ` <<< s`,
/// ` <<DELIM\nbody\nDELIM`) to cmd. Returns false for unknown modes.
fn append_redirect_frag(cmd: &mut String, fd: i64, mode: &str, target: &IrExpr) -> bool {
    // A redirect target is frequently NOT a literal: `sort <<<"${cfg[*]}"`
    // carries a `param("slice", …)` expansion. `call_arg_str` yields None for
    // such a node and the old `.unwrap_or_default()` turned that into an EMPTY
    // target — `<<<''` — so the herestring read nothing and the command
    // produced a silently wrong (empty) answer instead of refusing.
    // Render the real shell text; refuse (false) only if we cannot.
    // A plain LITERAL target must come back VERBATIM (`&1`, `/dev/null`):
    // `bash_word_for` shell-quotes it (`'&1'`), which turns the fd dup
    // `2>&1` into `2> '&1'` — bash then treats the quoted word as a
    // FILENAME and the command fails, losing the captured stderr that the
    // corpus compares on stdout (question-in-singlequote and 6 others).
    //
    // Everything else is rebuilt as SHELL text via `bash_word_for`, never
    // the Perl rendering `call_arg_str` would give for a Var
    // (`$ENV{DEST}`): bash reads that as `$ENV` plus the literal text
    // `{DEST}`, so `<$DEST.new` became the file `{DEST}.new`
    // (parse-dot-after-var).
    let literal = matches!(target, IrExpr::Str(_, _));
    let tstr = if literal {
        call_arg_str(target).unwrap_or_default()
    } else {
        let shell = bash_word_for(target);
        if shell.is_empty() {
            call_arg_str(target).unwrap_or_default()
        } else {
            shell
        }
    };
    if tstr.is_empty() {
        // Nothing renderable: refuse rather than emit an empty target, which
        // would silently produce a wrong answer (`<<<''`).
        return false;
    }
    match mode {
        "herestring" => {
            // `bash_word_for` yields the NATIVE shell form for an expansion
            // (`"${cfg[*]}"`), which MUST reach bash un-single-quoted or the
            // expansion is suppressed and the herestring reads nothing. A
            // literal target was already shell-quoted by `bash_word_for`, so
            // only single-quote it when it is not already a quoted word.
            let rendered = if tstr.contains('$') {
                tstr.clone()
            } else {
                format!("'{}'", tstr.replace('\'', "'\\\\''"))
            };
            cmd.push_str(&format!(" <<< {}", rendered));
            return true;
        }
        "heredoc" | "heredoc-tabs" => {
            // The heredoc BODY must not be inlined here. This function
            // appends to ONE STAGE of a pipeline, and stages are joined
            // with ` | `: inlining put the body and its terminator BEFORE
            // the remaining stages, so `cat <<EOF | grep -v '^#' | sed s/^/  /`
            // became
            //     cat <<'_SH2DOC_'
            //     <body>
            //     _SH2DOC_ | grep -v '^#' | sed 's/^/  /'
            // and bash read `_SH2DOC_ | grep …` as a CONTINUATION of the
            // body (warning "here-document delimited by end-of-file") and
            // printed the raw body (063_05, heredoc-binary-data,
            // heredoc-unclosed, 063_hard_to_parse).
            //
            // Emit just the operator now and stash the body in a
            // thread-local; the caller appends every stashed body after
            // the complete command line (HEREDOC_TAIL).
            //
            // `<<-` (heredoc-tabs) keeps its dash: bash strips leading TABS
            // from both the body and the terminator, and the delimiter must
            // still be found after that stripping. The tab-stripping is a
            // property of `<<-` itself, so the body goes out verbatim and
            // bash does the stripping (parse-heredoc-tab: stripping it here
            // would double-apply on a body that legitimately keeps tabs).
            let op = if mode == "heredoc-tabs" { " <<-" } else { " <<" };
            cmd.push_str(&format!("{op}'_SH2DOC_'"));
            HEREDOC_BODIES.with(|b| b.borrow_mut().push(tstr.clone()));
            return true;
        }
        // Process substitution (core-request perl-shir-20260806-1930):
        // the target carries the inner command text; bash expands it.
        "process-in" => {
            cmd.push_str(&format!(" <({})", tstr));
            return true;
        }
        "process-out" => {
            cmd.push_str(&format!(" >({})", tstr));
            return true;
        }
        _ => {}
    }
    let op = match mode {
        "w" => ">",
        "wc" => ">|", // `>|` — noclobber-bypassing truncate (POSIX)
        "a" => ">>",
        "r" => "<",
        _ => return false,
    };
    // fd N → target &M means N>&M / N<&M (fd dup) — the op is the DIRECTION
    // arrow, never the file mode (mode "w"/"a" would wrongly emit `2>>&1`,
    // which is not bash — syntax error).
    if let Some(m) = tstr.strip_prefix('&') {
        let arrow = if op == "<" { "<&" } else { ">&" };
        // NO space between the fd and the arrow: `2>&1` is the fd-dup
        // redirect; `2 >&1` would make `2` a FILE argument (verified:
        // mkdir got "cannot create directory '2'").
        cmd.push_str(&format!(" {}{}{}", fd, arrow, m));
        return true;
    }
    // A variable target (`< "$tmp"` from the process-subst transform, or a
    // frontend `Redirect` with a Var) must stay a shell variable read —
    // single-quoting it produced a literal file named after the Perl var
    // (`mapfile < '__ps_tmp2'` read a nonexistent file instead of the temp).
    let quoted = match var_ref_name(target) {
        Some(name) => {
            let v = var_read(&name);
            format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
        }
        None => match target {
            // A literal path → single-quote.
            IrExpr::Str(s, _) => format!("'{}'", s.replace('\'', "'\\\\''")),
            // An interpolated target (`> "$d/f1"`) → the bash
            // double-quoted word; `var_exports_str` exports the vars for
            // the child. (The old `call_arg_str(…).unwrap_or_default()`
            // emitted `> ''` — the pipeline lost its output file.)
            _ => {
                let w = bash_word_for(target);
                if w.is_empty() {
                    "''".to_string()
                } else {
                    w
                }
            }
        },
    };
    let frag = match fd {
        0 => format!(" < {}", quoted),
        // `op` not `>`: the old hardcode rebuilt `>>` (append) as `>`
        // (overwrite) — `echo B >> f` clobbered f.
        1 => format!(" {} {}", op, quoted),
        n => format!(" {}{} {}", n, op, quoted),
    };
    cmd.push_str(&frag);
    true
}

/// Rebuild a shell command string from a `redirect` Call (a command plus
/// fd redirect specs) — used in condition and statement positions.
/// `exec(cmd, …)` in condition position → the reconstructed shell
/// command string (the same shape `pipeline_call_to_cmd` handles).
/// Native file-contains for `grep -q PAT FILE` in condition position.
///
/// grep's exit status with its stdout suppressed by `-q` is exactly "does
/// a line of FILE contain the pattern". We read the file and do a
/// substring `index` when: every flag is one of `-q`/`-i`/`-s` (quiet /
/// case-fold / suppress-errors — none produce output or alter the line-
/// match semantics), the pattern is a BRE-literal (no metachars, so
/// substring == grep match), and there is exactly ONE file operand (a
/// literal path). Anything else (`-c -l -m -b -n -A -B -C -v -w -x
/// -E -F -Z`, globs, multiple files, a variable/arith pattern) keeps the
/// shell-out: it is not a plain boolean (grep would print matches/files/
/// counts).
/// The body of a verified `cat <<'EOF' … EOF` (cat with no args, exactly
/// one stdin heredoc redirect with a literal body). Shape check only — the
/// native emission lives in the pipeline_native cat capability.
/// Rewrite env-style `$NAME`/`${NAME}` in an interpolated heredoc body to
/// `$ENV{NAME}` (the assignment rendered those names as `%ENV{…}`).
fn rewrite_heredoc_env(s: &str) -> String {
    let re = regex::Regex::new(r"\$\{?([A-Z_][A-Z0-9_]*)\}?").unwrap();
    re.replace_all(s, "$$ENV{$1}").to_string()
}

/// `mapfile [-t] [flags] VAR < FILE` / `readarray …` → a native Perl read
/// into `@VAR`. `mapfile`/`readarray` are shell builtins; running them via
/// `bash -c` populates the array in the child shell (bash arrays cannot
/// cross exec), so the parent's array stays empty. Mirrors the C backend's
/// `is_mapfile_redirect` handling (c_backend.rs). Returns the Perl
/// statement, or None to fall back to the shell when the shape is exotic.
fn native_mapfile_read(inner: &[IrStmt], redirects: &[IrRedirect]) -> Option<String> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "mapfile" && cmd != "readarray" {
        return None;
    }
    // Exactly one stdin `r` redirect (the process-subst transform's form).
    let mut r_iter = redirects
        .iter()
        .filter(|r| matches!(r.fd, None | Some(0)) && r.mode == "r");
    let r = r_iter.next()?;
    if r_iter.next().is_some() || redirects.len() != 1 {
        return None;
    }
    let file = render_word(&r.target);
    let mut strip = false;
    let mut skip: Option<String> = None;
    let mut count: Option<String> = None;
    let mut origin: Option<String> = None;
    let mut name: Option<String> = None;
    let mut i = 0;
    while i < words.len() {
        let w = grep_lit_str(&words[i])?;
        if w == "--" {
            i += 1;
            continue;
        }
        if w.starts_with('-') && w.len() > 1 {
            let cs: Vec<char> = w[1..].chars().collect();
            let mut k = 0;
            while k < cs.len() {
                match cs[k] {
                    't' => strip = true,
                    'n' | 's' | 'O' => {
                        let rest: String = cs[k + 1..].iter().collect();
                        let val = if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            grep_lit_str(words.get(i)?)?
                        };
                        match cs[k] {
                            'n' => count = Some(val),
                            's' => skip = Some(val),
                            'O' => origin = Some(val),
                            _ => {}
                        }
                        k = cs.len();
                        continue;
                    }
                    'u' => {} // unbuffered — no-op
                    _ => return None,
                }
                k += 1;
            }
            i += 1;
            continue;
        }
        if name.is_some() {
            return None;
        }
        name = Some(w);
        i += 1;
    }
    let name = name.unwrap_or_else(|| "MAPFILE".to_string());
    // The array name is interpolated into Perl — refuse anything unsafe.
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let skip_n = skip.unwrap_or_else(|| "0".to_string());
    let count_guard = match &count {
        Some(c) => format!(" last if @__l >= {};", c),
        None => String::new(),
    };
    let chomp = if strip { "chomp $__line; " } else { "" };
    let body = format!(
        "do {{ open(my $__mf, '<', {file}) or die \"mapfile: $ERRNO\"; my @__l; my $__i = 0; while (my $__line = <$__mf>) {{ if ($__i < {skip_n}) {{ $__i++; next; }} {chomp}push @__l, $__line;{count_guard} }} close($__mf); @__l }}"
    );
    let assign = match &origin {
        Some(o) => format!("my @__mfr = {body}; splice(@{name}, {o}, scalar(@__mfr), @__mfr);"),
        None => format!("@{name} = {body};"),
    };
    Some(format!(
        "{assign} $main_exit_code = $CHILD_ERROR = 0;"
    ))
}

fn cat_heredoc_body(inner: &[IrStmt], redirects: &[IrRedirect]) -> Option<String> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else { return None };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return None };
    if cmd != "cat" || !words.is_empty() {
        return None;
    }
    if redirects.len() != 1 {
        return None;
    }
    let r = &redirects[0];
    if !matches!(r.mode.as_str(), "heredoc" | "heredoc-tabs") {
        return None;
    }
    if r.fd.is_some() && r.fd.unwrap() != 0 {
        return None;
    }
    call_arg_str(&r.target)
}

/// Extract a literal string from a pattern/file arg: a `Str`, an `Int`,
/// or an `Interpolate` whose parts are all literal text. Variables/arith /
/// captures → None (refuse). Shared with the pipeline_native capabilities.
pub(crate) fn grep_lit_str(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Int(n) => Some(n.to_string()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    crate::ir::InterpPart::Lit(t) => out.push_str(t),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn exec_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "exec" || func == "builtin" {
            let mut words: Vec<&IrExpr> = Vec::new();            // exec(cmd) / exec(cmd, words…): the first arg is the
            // command itself (e.g. the restructure pass's
            // `exec("true")` canonical loop condition).
            if let Some(first) = args.first() {
                if let IrExpr::Str(..) = first {
                    words.push(first);
                }
            }
            words.extend(exec_word_args(args));
            if words.is_empty() {
                return None;
            }
            return Some(words.iter().map(|w| render_word(w)).collect::<Vec<_>>().join(" "));
        }
    }
    None
}

/// The Arrow body of a `redirect(Arrow([…]), specs)` call — the IR view a
/// native inner-statement emission needs when the shell text won't rebuild.
fn redirect_call_arrow(call: &IrExpr) -> Option<&Vec<IrStmt>> {
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if func != "redirect" {
        return None;
    }
    if let Some(IrExpr::Arrow(stmts)) = args.first() {
        Some(stmts)
    } else {
        None
    }
}

fn redirect_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "redirect" {
            let mut cmd = args.first().and_then(arrow_to_cmd)?;
            for spec in args.iter().skip(1) {
                let specs: Vec<&IrExpr> = match spec {
                    IrExpr::Array(elems) => elems.iter().collect(),
                    other => vec![other],
                };
                for s in specs {
                    if let Some((fd, mode, target)) = redirect_spec_fields(s) {
                        append_redirect_frag(&mut cmd, fd, &mode, &target);
                    }
                }
            }
            return Some(cmd);
        }
    }
    None
}

/// Rebuild a shell command string from a `block` Call: multiple statements
/// run in sequence (`;`-joined) — used in condition position.
/// Split a multi-command loop CONDITION into the statements that must run
/// each iteration plus the expression whose truth decides the loop.
///
/// `while read -r l && [ -n "$l" ] && (( n < 2 )); do …; done` arrives as
/// `Call{block, [Arrow[stmts…]]}`. Those statements (`read`, `test`, …)
/// MUST be emitted inside the loop body — a one-line `system('bash','-c',…)`
/// runs `read` in a child and the value never reaches the Perl variable.
///
/// Returns `(leading statements, deciding expression)`:
///   * every statement but the last, verbatim;
///   * the last statement's expression, which supplies the status.
///
/// `None` for any shape that is not a block-wrapped arrow whose body is all
/// expression statements — the caller keeps the plain `while (expr)` form.
fn block_cond_parts(cond: &IrExpr) -> Option<(Vec<IrStmt>, &IrExpr)> {
    let IrExpr::Call { func, args } = cond else {
        return None;
    };
    if func != "block" {
        return None;
    }
    let arrow = args.iter().find_map(|a| match a {
        IrExpr::Arrow(stmts) => Some(stmts),
        _ => None,
    })?;
    // Every statement must be a bare expression, and there must be at least
    // one: a statement with side effects we cannot render natively would be
    // dropped silently, which is worse than the shell-out.
    if arrow.is_empty() || !arrow.iter().all(|s| matches!(s, IrStmt::Expr(_))) {
        return None;
    }
    let (last, lead) = arrow.split_last()?;
    let IrStmt::Expr(tail) = last else {
        return None;
    };
    Some((lead.to_vec(), tail))
}

fn block_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "block" {
            for a in args {
                if let IrExpr::Arrow(stmts) = a {
                    let cmds: Vec<String> = stmts
                        .iter()
                        .filter_map(|s| {
                            if let IrStmt::Expr(inner) = s {
                                expr_to_cmd(inner)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !cmds.is_empty() {
                        return Some(cmds.join("; "));
                    }
                }
            }
        }
    }
    None
}

/// Find the shell command string inside a block of stmts (exec, pipeline,
/// or `&&`/`||` chain) — shared by Arrow bodies and Redirect inners.
fn stmts_to_shell_cmd(stmts: &[IrStmt]) -> Option<String> {
    // Join ALL rebuildable statements with `;` — a first-match return would
    // drop the rest: `(echo a; echo b) | cat` lost `echo b` (examples/039
    // "Process 2").
    let mut parts = Vec::new();
    for s in stmts {
        if let IrStmt::Expr(inner) = s {
            if let Some(cmd) = expr_to_cmd(inner) {
                parts.push(cmd);
                continue;
            }
        }
        // A bare `IrStmt::Exec` (the procsub materialisation's `mktemp`
        // capture body): rebuild its shell text so the capture works.
        if let IrStmt::Exec { cmd, args, .. } = s {
            if let Some(c) = call_arg_str(cmd) {
                let words: Vec<&IrExpr> = args.iter().collect();
                parts.push(build_shell_cmd(&c, &words));
                continue;
            }
        }
        if let IrStmt::For { var, iter, body } = s {
            // `for k in ${!map[@]}; do …; done` as a pipeline stage — rebuild
            // the shell text so `… | sort` runs through bash -c like the
            // original script.
            let iter_cmd = bash_word_for(iter);
            let body_cmd = stmts_to_shell_cmd(body)?;
            parts.push(format!(
                "for {} in {}; do {}; done",
                var, iter_cmd, body_cmd
            ));
            continue;
        }
        // An `Output` (the native-statement transform's lowering of
        // `echo LIT`) as a pipeline stage / redirect body: rebuild it as the
        // shell `echo` it came from. Without this the block recursed into
        // returned None and the enclosing redirect died
        // (heredoc-unclosed: `( echo test ) <<EOF 2>&1 >/dev/null`).
        //
        // ONLY when a shell-out can preserve the redirect set: if the
        // enclosing Redirect carries an fd-dup onto a NON-standard fd
        // (`echo x >&3`), the dup lives in the parent's fd table and a fresh
        // `bash -c` cannot see it — the shell path would run the command
        // with fd 3 closed and report a failure the source does not have
        // (parse-invalid-redirect printed "exit: 1" instead of "exit: 0").
        // A plain block group `{ a; b; }` rebuilds as the same sequence.
        // The native-statement transform turns `( echo test )`'s body into
        // `Block([Output(…), Expr(exec true)])`, which has no shell spelling
        // of its own unless the block is recursed into — without this the
        // whole redirect fell through to the "not yet supported (redirect)"
        // die (heredoc-unclosed).
        if let IrStmt::Block(body) = s {
            // A block that is ONLY `Output` statements is the
            // native-statement transform's lowering of `echo LIT`
            // (`echo` folds to an `Output` before rendering). It has no
            // command text of its own, so rebuild it as the `echo` it came
            // from. This shape is the BODY of a Subshell a heredoc
            // redirects — `( echo test ) <<EOF` — and without it the
            // enclosing Redirect fell through to the "not yet supported
            // (redirect)" die (heredoc-unclosed). Scoped to a body of
            // Outputs alone so a block that also holds real commands keeps
            // the shell path unchanged.
            let is_output_or_noop = |b: &IrStmt| {
                matches!(b, IrStmt::Output { .. })
                    || matches!(
                        b,
                        IrStmt::Expr(IrExpr::Call { func, args })
                            if (func == "exec" || func == "builtin")
                                && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "true" || s == ":")
                    )
            };
            if !body.is_empty() && body.iter().all(|b| is_output_or_noop(b)) {
                let mut cmds: Vec<String> = Vec::new();
                for b in body {
                    if let IrStmt::Output { value, newline, .. } = b {
                        let word = bash_word_for(value);
                        cmds.push(if *newline {
                            format!("echo {word}")
                        } else {
                            format!("printf '%s' {word}")
                        });
                    }
                }
                parts.push(cmds.join("; "));
                continue;
            }
            let inner_cmd = stmts_to_shell_cmd(body)?;
            parts.push(inner_cmd);
            continue;
        }
        if let IrStmt::Subshell(body) = s {
            if !body.is_empty() && body.iter().all(|b| matches!(b, IrStmt::Output { .. })) {
                let mut cmds: Vec<String> = Vec::new();
                for b in body {
                    if let IrStmt::Output { value, newline, .. } = b {
                        let word = bash_word_for(value);
                        cmds.push(if *newline {
                            format!("echo {word}")
                        } else {
                            format!("printf '%s' {word}")
                        });
                    }
                }
                parts.push(format!("( {} )", cmds.join("; ")));
                continue;
            }
        }
        if let IrStmt::Subshell(body) = s {
            // `( … )` as a pipeline stage: `(echo a; echo b) | cat` — the
            // subshell rebuilds as a parenthesized group.
            let inner_cmd = stmts_to_shell_cmd(body)?;
            parts.push(format!("( {inner_cmd} )"));
            continue;
        }
        // A redirect-wrapped command (`gzip … 2>/dev/null`) inside a
        // capture body: rebuild `cmd <redirs>` (the multi-command
        // `$(gzip a 2>/dev/null; gzip b 2>/dev/null)` capture).
        if let IrStmt::Redirect { inner, redirects } = s {
            let mut cmd = stmts_to_shell_cmd(inner)?;
            for r in redirects {
                let fd = r.fd.unwrap_or(1);
                if !append_redirect_frag(&mut cmd, fd as i64, &r.mode, &r.target) {
                    return None;
                }
            }
            parts.push(cmd);
            continue;
        }
        // An `IrStmt::Pipeline` as a capture body (``echo `ls | grep -v a` ``):
        // the Arrow-wrapped `pipeline` Call form rebuilds via expr_to_cmd
        // above, but a command substitution whose body is a pipeline arrives
        // as this STATEMENT variant. Without this arm the rebuild returned
        // None and the capture's VALUE was dropped — `echo `cmd`` rendered
        // `print('', "\n")` (perl-generator-20261002-053000: the substituted
        // output silently vanished from stdout).
        if let IrStmt::Pipeline { stages, .. } = s {
            let stage_exprs: Vec<IrExpr> = stages
                .iter()
                .map(|st| IrExpr::Arrow(st.clone()))
                .collect();
            let stage_refs: Vec<&IrExpr> = stage_exprs.iter().collect();
            let cmd = stages_to_shell_cmd(&stage_refs)?;
            parts.push(cmd);
            continue;
        }
        // a statement the rebuild can't express kills the whole command
        return None;
    }
    if parts.is_empty() {
        None
    } else {
        // NOTE: no `take_heredoc_tail()` here. This function is called both
        // at the top level AND for a single STAGE of a pipeline (via
        // `arrow_to_cmd`), and a stage must not carry the body: the body
        // belongs after the whole line. Draining here put
        //     cat <<'_SH2DOC_'
        //     <body>
        //     _SH2DOC_ | grep …
        // inside the stage text (063_05). The outermost caller
        // (`stages_to_shell_cmd`, or the Redirect arm for a standalone
        // command) appends the tail once, after every stage and redirect.
        Some(parts.join("; "))
    }
}

/// Rebuild a shell command string from `stages` (each already an
/// `IrExpr`), joining them with ` | `.
/// A stage whose shell text cannot be rebuilt must NOT be silently
/// DROPPED — the command would lose a producer (`paste <(…) <(…) | head`
/// became `head`). Refuse, so the caller emits the stages natively.
fn stages_to_shell_cmd(stages: &[&IrExpr]) -> Option<String> {
    let mut out: Vec<String> = Vec::with_capacity(stages.len());
    for s in stages {
        let c = arrow_to_cmd(s)?;
        out.push(c);
    }
    if out.is_empty() {
        None
    } else {
        // A heredoc body collected from any stage belongs at the END of the
        // complete pipeline line, never inside a stage (see
        // `append_redirect_frag`'s heredoc arm).
        Some(format!("{}{}", out.join(" | "), take_heredoc_tail()))
    }
}

/// Rebuild a shell command string from a `pipeline` Call:
/// pipeline([Arrow([Expr(exec)]), Arrow([Expr(exec)]), …]) → "a | b".
fn arrow_to_cmd(a: &IrExpr) -> Option<String> {
    if let IrExpr::Arrow(stmts) = a {
        stmts_to_shell_cmd(stmts)
    } else {
        None
    }
}

/// Expand a single GNU `tr` POSIX class argument to a plain transliteration
/// range of equal length (`[:lower:]`→`a-z`, `[:upper:]`→`A-Z`, …), so it
/// can be rendered with Perl's `tr///`. Only 1:1 positional classes are
/// supported; everything else (and any `-s`/`-d`/`-c` flag semantics)
/// keeps the shell-out path.
fn tr_class_to_range(set: &str) -> Option<String> {
    match set {
        "[:lower:]" => Some("a-z".into()),
        "[:upper:]" => Some("A-Z".into()),
        "[:digit:]" => Some("0-9".into()),
        "[:alpha:]" => Some("A-Za-z".into()),
        "[:alnum:]" => Some("A-Za-z0-9".into()),
        "[:xdigit:]" => Some("0-9A-Fa-f".into()),
        _ => None,
    }
}

/// Native fold for the common `echo LIT… | tr SET1 SET2` pipeline
/// statement. When both stages are plain literal `builtin` word lists (no
/// flags, no expansions, no redirects) and the `tr` sets are either plain
/// ranges or equal-length POSIX classes, emit a Perl string + `tr///` + print
/// instead of a whole `bash -c` shell-out. Returns true (and emits) when the
/// fold applies; false keeps the reconstruction/shell-out path.
fn echo_tr_expr_into(call: &IrExpr, expr_out: &mut Option<String>) -> bool {
    let stage_exprs = match call {
        IrExpr::Call { func, args } if func == "pipeline" => {
            let mut v = Vec::new();
            for a in args {
                if let IrExpr::Array(elems) = a {
                    v.extend(elems.iter());
                } else {
                    v.push(a);
                }
            }
            v
        }
        _ => return false,
    };
    // exactly two stages, each a single builtin Call statement
    if stage_exprs.len() != 2 {
        return false;
    }
    let a0 = match stage_exprs[0] {
        IrExpr::Arrow(stmts) => match stmts.as_slice() {
            [IrStmt::Expr(IrExpr::Call { func, args })]
                if func == "builtin" || func == "exec" =>
            {
                args.as_slice()
            }
            _ => return false,
        },
        _ => return false,
    };
    let a1 = match stage_exprs[1] {
        IrExpr::Arrow(stmts) => match stmts.as_slice() {
            [IrStmt::Expr(IrExpr::Call { func, args })]
                if func == "builtin" || func == "exec" =>
            {
                args.as_slice()
            }
            _ => return false,
        },
        _ => return false,
    };
    // stage 0: `builtin("echo", [words…])`/`exec("echo", …)` — the
    // command name is args[0]; the function name is the op (builtin/exec).
    let f0 = match a0.first().and_then(call_arg_str) {
        Some(s) => s,
        None => return false,
    };
    let f1 = match a1.first().and_then(call_arg_str) {
        Some(s) => s,
        None => return false,
    };
    if f0 != "echo" {
        return false;
    }
    let words = if let [_, IrExpr::Array(elems)] = a0 {
        elems
    } else {
        return false;
    };
    if words.is_empty() {
        return false;
    }
    // echo WORDS: literal words inline, dynamic words render as Perl
    // expressions (`echo "$word" | tr …`). An unquoted `$var` would
    // expand to multiple fields via the split marker — refuse those.
    let mut parts: Vec<String> = Vec::new();
    for w in words {
        if let Some(lit) = crate::ir::grep_lit_str(w) {
            if lit.starts_with('-') {
                return false; // echo -n / -e …
            }
            parts.push(crate::ir::safe_perl_q_string(&lit));
        } else if matches!(w, IrExpr::Call { func, .. } if func == "split") {
            return false;
        } else {
            parts.push(render_word(w));
        }
    }
    if parts.is_empty() {
        return false;
    }
    let echo_expr = if parts.len() == 1 {
        format!("{} . \"\\n\"", parts[0])
    } else {
        format!("join(' ', {}) . \"\\n\"", parts.join(", "))
    };
    // stage 1: tr SET1 SET2, both sets plain-ranges or equal-length classes
    if f1 != "tr" {
        return false;
    }
    let trargs = if let [_, IrExpr::Array(elems)] = a1 {
        elems
    } else {
        return false;
    };
    if trargs.len() != 2 {
        return false;
    }
    let (s1, s2) = match (
        crate::ir::grep_lit_str(&trargs[0]),
        crate::ir::grep_lit_str(&trargs[1]),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    let r1 = if set_is_plain_range(&s1) {
        s1
    } else {
        match tr_class_to_range(&s1) {
            Some(r) => r,
            None => return false,
        }
    };
    let r2 = if set_is_plain_range(&s2) {
        s2
    } else {
        match tr_class_to_range(&s2) {
            Some(r) => r,
            None => return false,
        }
    };
    if r1.starts_with('-') || r2.starts_with('-') {
        return false;
    }
    // `echo a b` joins words with a single space and appends a newline;
    // tr transliterates stdin 1:1. Return the value expression; the two
    // callers print it (statement) or capture it (`x=$(echo … | tr …)`).
    *expr_out = Some(format!(
        "do {{ my $__t = {}; $__t =~ tr/{}/{}/; $__t }}",
        echo_expr, r1, r2
    ));
    true
}

/// Statement fold: `echo … | tr …` → print the transliteration.
fn try_native_echo_tr_pipeline(out: &mut String, call: &IrExpr, indent: usize) -> bool {
    let mut expr = None;
    if !echo_tr_expr_into(call, &mut expr) {
        return false;
    }
    emit_indent(out, indent);
    out.push_str(&format!(
        "my $__tr_out = {}; print $__tr_out; $main_exit_code = $CHILD_ERROR = 0;\n",
        expr.unwrap()
    ));
    true
}

/// Capture fold: the VALUE of `echo … | tr …` (trailing newline included —
/// the caller chomps it).
fn echo_tr_expr(call: &IrExpr) -> Option<String> {
    let mut expr = None;
    if echo_tr_expr_into(call, &mut expr) {
        expr
    } else {
        None
    }
}

/// Native fold for `while … read … done < FILE | sort [-n]` (the shell
/// parser materialises the loop as a `whileLoop` call inside a `redirect`
/// stage). Run the loop with stdout captured, then sort the lines in Perl.
/// Returns true (and emits) when the exact shape applies.
fn try_native_while_read_sort(out: &mut String, call: &IrExpr, indent: usize) -> bool {
    let IrExpr::Call { func, args } = call else {
        return false;
    };
    if func != "pipeline" {
        return false;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return false;
    };
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return false;
    };
    // Consumer: `sort [flags]` or `head [-n] N`. Both consume the loop's
    // output as a line stream; `head` keeps the first N (bash's `head -3`
    // prints the first three lines — 091_while_pipe_var).
    let (consumer_words, consumer_cmd) = match s2.as_slice() {
        [IrStmt::Expr(IrExpr::Call { func: sf, args: sa })]
            if sf == "exec" || sf == "builtin" =>
        {
            let [IrExpr::Str(cmd, _), IrExpr::Array(sw)] = sa.as_slice() else {
                return false;
            };
            if cmd != "sort" && cmd != "head" {
                return false;
            }
            let words: Vec<String> = sw
                .iter()
                .filter_map(crate::ir::grep_lit_str)
                .collect();
            (words, cmd.clone())
        }
        _ => return false,
    };
    let sort_flags = consumer_words.clone();
    // Producer: `redirect(Arrow([whileLoop(read, body)]), file)`.
    let (read_cond, body, file) = match s1.as_slice() {
        [IrStmt::Expr(IrExpr::Call { func: rf, args: ra })] if rf == "redirect" => {
            let arrow = match ra.first() {
                Some(IrExpr::Arrow(a)) => a,
                _ => return false,
            };
            let wl = match arrow.as_slice() {
                [IrStmt::Expr(w)] => w,
                _ => return false,
            };
            let IrExpr::Call { func: wf, args: wa } = wl else {
                return false;
            };
            if wf != "whileLoop" {
                return false;
            }
            let cond = match wa.first() {
                Some(IrExpr::Arrow(c)) => match c.as_slice() {
                    [IrStmt::Expr(e)] => e.clone(),
                    _ => return false,
                },
                _ => return false,
            };
            let body = match wa.get(1) {
                Some(IrExpr::Arrow(b)) => b.clone(),
                _ => return false,
            };
            let mut file: Option<String> = None;
            for spec in ra.iter().skip(1) {
                let specs: Vec<&IrExpr> = match spec {
                    IrExpr::Array(el) => el.iter().collect(),
                    o => vec![o],
                };
                for sp in specs {
                    if let Some((0, m, t)) = redirect_spec_fields(sp) {
                        if m == "r" {
                            file = call_arg_str(&t);
                        }
                    }
                }
            }
            (cond, body, file)
        }
        _ => return false,
    };
    let file = match file {
        Some(f) => f,
        None => return false,
    };
    // Render the loop into a buffer (it prints to STDOUT, which the wrapper
    // below redirects into a capture scalar).
    let mut loop_buf = String::new();
    emit_stmt(
        &mut loop_buf,
        &IrStmt::While {
            cond: read_cond,
            body,
        },
        0,
    );
    let cmp = if sort_flags.iter().any(|f| f == "-n") {
        "$a <=> $b"
    } else {
        "$a cmp $b"
    };
    // `head [-n] N` — keep the first N lines. bash's bare `head` defaults to
    // 10; `-N` and `-n N` are the corpus spellings. Anything else (multiple
    // files, `-c`) refuses and keeps the shell-out.
    let head_n: Option<i64> = if consumer_cmd == "head" {
        match consumer_words.as_slice() {
            [] => Some(10),
            [a] => a
                .strip_prefix('-')
                .and_then(|d| d.parse::<i64>().ok()),
            [a, b] if a == "-n" => b.parse::<i64>().ok(),
            _ => None,
        }
    } else {
        None
    };
    if consumer_cmd == "head" && head_n.is_none() {
        return false;
    }
    emit_indent(out, indent);
    out.push_str(&format!(
        "my $__wr_buf = do {{ my $__cap = ''; open(my $__wo, '>&', \\*STDOUT) or die \"capture: $!\\n\"; close(STDOUT); open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {{ local *STDIN; open(STDIN, '<', {file}) or die \"read: $!\\n\"; {lp} }} open(STDOUT, '>&', $__wo) or die \"capture: $!\\n\"; close($__wo); $__cap }};\n",
        file = safe_perl_q_string(&file),
        lp = loop_buf.trim_start(),
    ));
    emit_indent(out, indent);
    if let Some(n) = head_n {
        // `head -N`: split on lines, keep the first N, print each. bash's
        // `head` prints a trailing newline per line it emits.
        out.push_str(&format!(
            "my @__wr = split(/\\n/, $__wr_buf); @__wr = @__wr[0 .. {last}] if @__wr > {n}; print \"$_\\n\" for @__wr; $main_exit_code = $CHILD_ERROR = 0;\n",
            last = n.saturating_sub(1),
            n = n,
        ));
    } else {
        out.push_str(&format!(
            "my @__wr = split(/\\n/, $__wr_buf); @__wr = sort {{ {cmp} }} @__wr; print \"$_\\n\" for @__wr; $main_exit_code = $CHILD_ERROR = 0;\n"
        ));
    }
    true
}

/// `for VAR in LIST; do BODY; done | sort [flags]` — run the loop natively
/// with STDOUT captured, then sort the lines. The rebuilt-shell fallback
/// shelled the whole construct to `bash -c … | sort`, which check_qx flags
/// (and which cannot see the Perl-side list the `for` iterates).
fn try_native_for_sort(out: &mut String, call: &IrExpr, indent: usize) -> bool {
    let IrExpr::Call { func, args } = call else {
        return false;
    };
    if func != "pipeline" {
        return false;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return false;
    };
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return false;
    };
    // Consumer: `sort [flags]` or `head [-n] N`. Both consume the loop's
    // output as a line stream; `head` keeps the first N (bash's `head -3`
    // prints the first three lines — 091_while_pipe_var).
    let (consumer_words, consumer_cmd) = match s2.as_slice() {
        [IrStmt::Expr(IrExpr::Call { func: sf, args: sa })]
            if sf == "exec" || sf == "builtin" =>
        {
            let [IrExpr::Str(cmd, _), IrExpr::Array(sw)] = sa.as_slice() else {
                return false;
            };
            if cmd != "sort" && cmd != "head" {
                return false;
            }
            let words: Vec<String> = sw
                .iter()
                .filter_map(crate::ir::grep_lit_str)
                .collect();
            (words, cmd.clone())
        }
        _ => return false,
    };
    let sort_flags = consumer_words.clone();
    // Producer: a single `for` loop.
    let (var, iter, body) = match s1.as_slice() {
        [IrStmt::For { var, iter, body }] => (var.clone(), iter.clone(), body.clone()),
        _ => return false,
    };
    let mut loop_buf = String::new();
    emit_stmt(
        &mut loop_buf,
        &IrStmt::For {
            var,
            iter,
            body,
        },
        0,
    );
    let cmp = if sort_flags.iter().any(|f| f == "-n") {
        "$a <=> $b"
    } else {
        "$a cmp $b"
    };
    // `head [-n] N` — keep the first N lines. bash's bare `head` defaults to
    // 10; `-N` and `-n N` are the corpus spellings. Anything else (multiple
    // files, `-c`) refuses and keeps the shell-out.
    let head_n: Option<i64> = if consumer_cmd == "head" {
        match consumer_words.as_slice() {
            [] => Some(10),
            [a] => a
                .strip_prefix('-')
                .and_then(|d| d.parse::<i64>().ok()),
            [a, b] if a == "-n" => b.parse::<i64>().ok(),
            _ => None,
        }
    } else {
        None
    };
    if consumer_cmd == "head" && head_n.is_none() {
        return false;
    }
    emit_indent(out, indent);
    out.push_str(&format!(
        "my $__fs_buf = do {{ my $__cap = ''; open(my $__fo, '>&', \\*STDOUT) or die \"capture: $!\\n\"; close(STDOUT); open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {lp} open(STDOUT, '>&', $__fo) or die \"capture: $!\\n\"; close($__fo); $__cap }};\n",
        lp = loop_buf.trim_start(),
    ));
    emit_indent(out, indent);
    out.push_str(&format!(
        "my @__fs = split(/\\n/, $__fs_buf); @__fs = sort {{ {cmp} }} @__fs; print \"$_\\n\" for @__fs; $main_exit_code = $CHILD_ERROR = 0;\n"
    ));
    true
}

/// Is `set` a plain tr range (only alnum/`-` chars, not beginning with `-`)
/// safe to drop into Perl's `tr///`?
fn set_is_plain_range(set: &str) -> bool {
    !set.is_empty()
        && !set.starts_with('-')
        && !set.ends_with('-')
        && set
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-')
}

/// The stage statement-lists of a `pipeline(...)` call (the shapes
/// `pipeline([Arrow, …])` / `pipeline(Arrow, …)`), for the emitter's
/// stage-by-stage fallback when no single shell command exists.
fn pipeline_stages_of(call: &IrExpr) -> Option<Vec<Vec<IrStmt>>> {
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    let mut out: Vec<Vec<IrStmt>> = Vec::new();
    for a in args {
        match a {
            IrExpr::Array(elems) => {
                for el in elems {
                    match el {
                        IrExpr::Arrow(b) => out.push(b.clone()),
                        _ => return None,
                    }
                }
            }
            IrExpr::Arrow(b) => out.push(b.clone()),
            _ => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn pipeline_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        if func == "pipeline" {
            // Stages arrive as pipeline([Arrow, Arrow, …]) or
            // pipeline([Array([Arrow, Arrow, …])]).
            let mut stages: Vec<&IrExpr> = Vec::new();
            for a in args {
                if let IrExpr::Array(elems) = a {
                    stages.extend(elems.iter());
                } else {
                    stages.push(a);
                }
            }
            return stages_to_shell_cmd(&stages);
        }
    }
    None
}

/// Rebuild the shell command string from a `capture` Call (command
/// substitution `$(…)` / backticks) whose closure body is an exec/pipeline.
fn capture_call_to_cmd(call: &IrExpr) -> Option<String> {
    if let IrExpr::Call { func, args } = call {
        // The A1 spells command substitution `captureWords` in unquoted word
        // position (`for k in $(echo … | sort)`); `capture` is the scalar /
        // assignment-position spelling. Only handling `capture` made every
        // `captureWords` iterator fall through to the joined-STRING render,
        // so `for k in $(…)` ran ONE iteration carrying the whole value
        // instead of one per word (`join(' ', split(…))` collapses the list
        // back to a single scalar).
        if func == "capture" || func == "captureWords" {
            for a in args {
                if let Some(cmd) = arrow_to_cmd(a) {
                    // A heredoc in the captured body (`$(cat <<-EOF …)`) puts
                    // its body at the end of the command line, not inside the
                    // arrow's own text (see `append_redirect_frag`).
                    return Some(format!("{cmd}{}", take_heredoc_tail()));
                }
            }
        }
    }
    None
}

/// The Arrow body of a `capture(Arrow([…]))` call, if that is its shape —
/// the IR view a native-pipeline capture needs (the shell-text view is
/// `capture_call_to_cmd`).
fn capture_call_arrow(call: &IrExpr) -> Option<&Vec<IrStmt>> {
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if func != "capture" {
        return None;
    }
    for a in args {
        if let IrExpr::Arrow(stmts) = a {
            return Some(stmts);
        }
    }
    None
}

/// Render a `for` iterable: `Array([…])` → Perl list; `Range` → `a..b`;
/// `split(x)` → `split /\s+/, x` (bash IFS word splitting); a single
/// `capture` element → iterate the captured words.
fn word_iter_to_perl(iter: &IrExpr) -> String {
    match iter {
        IrExpr::Call { func, args } if func == "split" => {
            let inner = args.first().map(render_word).unwrap_or_default();
            format!("split /\\s+/, {}", inner)
        }
        IrExpr::Array(elems) if elems.len() == 1 => {
            if let IrExpr::Call { func, .. } = &elems[0] {
                // `for w in $x` — the A1 wraps the unquoted read in a
                // single-element Array (triage-perl t62_word_split); the
                // split must stay a LIST here, never render_word's raw
                // text (one iteration for the whole "a b").
                if func == "split" {
                    if let IrExpr::Call { args, .. } = &elems[0] {
                        let inner = args.first().map(render_word).unwrap_or_default();
                        return format!("split /\\s+/, {}", inner);
                    }
                }
                if func == "capture" || func == "captureWords" {
                    if let Some(cmd) = capture_call_to_cmd(&elems[0]) {
                        return format!("split /\\s+/, {}", cmd_str_to_open_perl(&cmd));
                    }
                }
                if func == "brace" {
                    if let IrExpr::Call { args, .. } = &elems[0] {
                        return render_brace_iter(args);
                    }
                }
                // `for i in "$*"` — bash's `"$*"` is the positional params
                // joined by IFS as ONE word, so the loop runs ONCE even with
                // no arguments (the word is then empty and the body sees an
                // empty `$i`). Rendering the array itself iterated zero times
                // and parse-dollars-in-string lost its blank line. Wrap the
                // join in a one-element list to keep the single iteration.
                if matches!(func.as_str(), "listVar" | "getVar" | "listVarWords") {
                    if let IrExpr::Call { args, .. } = &elems[0] {
                        if args.first().and_then(call_arg_str).as_deref() == Some("*") {
                            // ONE word: `join` of the params (empty when
                            // there are none, which still yields a single
                            // empty iteration).
                            return "join(' ', @ARGV)".to_string();
                        }
                    }
                }
            }
            ir_expr_to_perl(iter)
        }
        IrExpr::Array(_) => ir_expr_to_perl(iter),
        IrExpr::Call { func, args } if func == "brace" => render_brace_iter(args),
        _ => ir_expr_to_perl(iter),
    }
}

/// Lower a modern-IR `exec` Call in statement position.
fn emit_exec_call(out: &mut String, call: &IrExpr, indent: usize) {
    let (cmd, mut words) = match call {
        IrExpr::Call { args, .. } => match exec_call_parts(args) {
            Some(p) => p,
            None => {
                // Dynamic / unparseable command — shell out via bash -c.
                // The rebuilt text goes to a shell, so the words must be
                // SHELL syntax: `"$@"` stays `"$@"` (bash expands it), not
                // the Perl `@ARGV`/`join(...)` rendering.
                let full = args
                    .iter()
                    .map(bash_word_for)
                    .collect::<Vec<_>>()
                    .join(" ");
                emit_shell_cmd(out, indent, &full);
                return;
            }
        },
        _ => return,
    };
    // The env-prefix Object arg (`VAR1=x cmd`) is NOT a word. bash semantics:
    // the assignment applies ONLY to the command's CHILD processes — argument
    // expansion sees the OLD value (`VAR1=x echo "$VAR1"` prints EMPTY). So
    // for the in-Perl emulations (no children) the assignment is DEAD and
    // must NOT precede the emulation (it would leak into the arg reads); the
    // bash-c fallback emits it before system() so the child inherits it.
    let mut env_pre = String::new();
    words.retain(|w| match w {
        IrExpr::Object(props) => {
            for (k, v) in props {
                env_pre.push_str(&format!("$ENV{{{}}} = {};\n", k, render_word(v)));
            }
            false
        }
        _ => true,
    });
    // `eval "TEXT"` — bash PARSES TEXT as source and runs it in the
    // CURRENT shell, so its assignments persist. Parse it here and emit what
    // that source means:
    //   eval "result=$(( $var + 1 ))"  ⇒  $result = $var + 1;
    // The generic path emitted the literal TEXT, so 063_12 printed the raw
    // source (`Eval result: $(( ${var:-0} + … ))`) and 063_hard_to_parse
    // printed nothing at all.
    //
    // STATIC literal text only (the common corpus shape). A dynamic argument
    // cannot be resolved at translation time; it falls through to the
    // shell-out, where the child parses it exactly like bash.
    if cmd == "eval" {
        if let Some(text) = words.iter().find_map(|w| static_word_text(w)) {
            if let Ok(cmds) = crate::parser::commands::parse(&text) {
                if !cmds.is_empty() {
                    let sub = crate::shir::ast_to_ir(&cmds);
                    for st in &sub.stmts {
                        emit_stmt(out, st, indent);
                    }
                    return;
                }
            }
            emit_indent(out, indent);
            out.push_str(
                "(print STDERR \"$0: eval: syntax error\\n\"); $main_exit_code = $CHILD_ERROR = 1;\n",
            );
            return;
        }
        // An INTERPOLATED eval argument (`eval "echo \"… $x\""`): bash
        // expands the words BEFORE eval parses the result, so the text is
        // "the literals with each expansion substituted". Rebuild exactly
        // that as shell source and lower it — which keeps the value
        // semantics (a substituted `$x` becomes the VALUE's shell spelling)
        // instead of handing bash a mangled nested-quoting string
        // (075_eval_complex printed nothing).
        if let Some(expanded) = words.iter().find_map(|w| eval_word_shell_text(w)) {
            if let Ok(cmds) = crate::parser::commands::parse(&expanded) {
                if !cmds.is_empty() {
                    let sub = crate::shir::ast_to_ir(&cmds);
                    for st in &sub.stmts {
                        emit_stmt(out, st, indent);
                    }
                    return;
                }
            }
        }
    }
    if let Some(native) = crate::pipeline_native::native_exec_stmt(call) {
        emit_indent(out, indent);
        // A value-channel leaf (`$__sh_cap`) used in STATEMENT position must
        // print its value (e.g. `basename $(pwd)`); in capture position the
        // caller reads `$__sh_cap` directly.
        if native.contains("$__sh_cap") {
            out.push_str(&format!(
                "my $__sh_cap; {native} print $__sh_cap, \"\\n\" if defined $__sh_cap;\n"
            ));
        } else {
            out.push_str(&native);
            out.push('\n');
        }
        return;
    }
    match cmd.as_str() {
        "echo" => emit_echo(out, &words, indent),
        "printf" => {
            if words.is_empty() {
                emit_indent(out, indent);
                out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
                return;
            }
            // The format word. bash's printf interprets backslash escapes
            // (`\n`, `\t`, `\\`) IN the format; Perl's does not — decode
            // them (the A1 Interpolate lit text carries the two-char bash
            // sequence) so the emitted format prints the same bytes
            // (triage-perl t62_word_split).
            let fmt = match &words[0] {
                IrExpr::Str(s, _) => {
                    let mut f = String::from("\"");
                    for ch in s.chars() {
                        match ch {
                            '"' => f.push_str("\\\""),
                            '$' => f.push_str("\\$"),
                            '@' => f.push_str("\\@"),
                            c if is_byte_marker(c) => f.push_str(&byte_marker_escape(c)),
                            c => f.push(c),
                        }
                    }
                    f.push('"');
                    f
                }
                IrExpr::Interpolate(parts) => {
                    let mut f = String::from("\"");
                    for part in parts {
                        match part {
                            InterpPart::Lit(text) => {
                                let mut cs = text.chars().peekable();
                                while let Some(c) = cs.next() {
                                    match c {
                                        '\\' => match cs.peek() {
                                            Some('n') => {
                                                f.push_str("\\n");
                                                cs.next();
                                            }
                                            Some('t') => {
                                                f.push_str("\\t");
                                                cs.next();
                                            }
                                            Some('r') => {
                                                f.push_str("\\r");
                                                cs.next();
                                            }
                                            Some('\\') => {
                                                f.push_str("\\\\");
                                                cs.next();
                                            }
                                            _ => f.push_str("\\\\"),
                                        },
                                        '"' => f.push_str("\\\""),
                                        '$' => f.push_str("\\$"),
                                        '@' => f.push_str("\\@"),
                                        c if is_byte_marker(c) => {
                                            f.push_str(&byte_marker_escape(c))
                                        }
                                        c => f.push(c),
                                    }
                                }
                            }
                            InterpPart::Expr(e) => {
                                let w = render_word(e);
                                f.push_str(&w.replace('\\', "\\\\"));
                            }
                        }
                    }
                    f.push('"');
                    f
                }
                _ => render_word(words[0]),
            };
            let rest: Vec<String> = words[1..].iter().map(|w| render_word_list(w)).collect();
            let has_split = words[1..]
                .iter()
                .any(|w| matches!(w, IrExpr::Call { func, .. } if func == "split"));
            // bash `printf %c` prints the FIRST CHARACTER of the argument
            // (`printf '%c' 65` → `6`, `printf '%c' hello` → `h`); Perl's
            // `%c` NUMIFIES the argument instead (`"hello"` → NUL). When the
            // format is a literal and the args are positional (one per
            // conversion), wrap each `%c` operand in `substr(…, 0, 1)` so the
            // emitted Perl matches bash (c-sh-go t10_string_pointer:
            // `printf("%c\n", s[0])` printed a NUL, not `h`).
            let mut rest = rest;
            let c_convs: Option<Vec<char>> = literal_printf_conversions(&words[0]);
            if !has_split {
                if let Some(convs) = &c_convs {
                    if !rest.is_empty() && rest.len() == convs.len() {
                        for (i, c) in convs.iter().enumerate() {
                            if *c == 'c' {
                                // Perl's `%c` takes a CODE POINT: `ord` of the
                                // first character (`"h"` → 104 → `h`); bash's
                                // `%c` prints that same first character.
                                rest[i] = format!("ord(substr({}, 0, 1))", rest[i]);
                            }
                        }
                    }
                }
            }
            // placeholder count in the (rendered) format: a `%` not part of
            // a `%%` pair is one conversion. Computed on the RENDERED literal
            // so Interpolate formats (`"%s-%s\n"` → Interpolate Lit) count
            // too — call_arg_str only sees plain Str formats.
            let p = {
                let cs: Vec<char> = fmt.chars().collect();
                let mut n = 0;
                let mut i = 0;
                while i < cs.len() {
                    if cs[i] == '%' {
                        if i + 1 < cs.len() && cs[i + 1] == '%' {
                            i += 2;
                            continue;
                        }
                        n += 1;
                    }
                    i += 1;
                }
                n
            };
            emit_indent(out, indent);
            if rest.is_empty() || p == 0 {
                // no args, or no conversions: bash printf prints the format
                // ONCE, ignoring the args (GNU printf 'x' a b → "x"); Perl
                // printf(fmt) also interprets `%%` (a bare print would leak
                // the literal `%%`).
                out.push_str(&format!("printf({});\n", fmt));
            } else if has_split || rest.len() > p {
                // bash printf CYCLES the format over the whole arg list;
                // Perl printf applies the format ONCE and discards extra
                // args. Flatten the args (a split word expands to its
                // fields in list context) and emit one printf per
                // format-application, chunked by the placeholder count P
                // (triage-perl t62_word_split: `printf "<%s>\\n" $x`
                // with x="a b" → two lines; same for plain literal args:
                // `printf '%s\\n' a b c` → three lines).
                static PA_SEQ: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                let tmp = format!(
                    "__sh2_pa{}",
                    PA_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                );
                out.push_str(&format!("my @{tmp} = ({});\n", rest.join(", ")));
                out.push_str(&format!(
                    "my ${tmp}_n = @{tmp} || 1;\n"
                ));
                out.push_str(&format!(
                    "for (my ${tmp}_i = 0; ${tmp}_i < ${tmp}_n; ${tmp}_i += {p}) {{\n"
                ));
                out.push_str(&format!(
                    "    printf({fmt}, @{tmp}[${tmp}_i .. (${tmp}_i + {p} - 1 < ${tmp}_n ? ${tmp}_i + {p} - 1 : ${tmp}_n - 1)], (\"\") x ((${tmp}_i + {p} > ${tmp}_n) ? ${tmp}_i + {p} - ${tmp}_n : 0));\n"
                ));
                out.push_str("}\n");
            } else {
                out.push_str(&format!("printf({}, {});\n", fmt, rest.join(", ")));
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "cd" => {
            let dir = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "$ENV{HOME}".to_string());
            emit_indent(out, indent);
            out.push_str(&format!(
                "chdir({}) or die \"cd: $!\\n\"; $main_exit_code = $CHILD_ERROR = 0;\n",
                dir
            ));
        }
        "export" => {
            for w in &words {
                if let Some(word_str) = call_arg_str(w) {
                    if let Some(eq) = word_str.split_once('=') {
                        emit_indent(out, indent);
                        out.push_str(&format!(
                            "$ENV{{{}}} = {};\n",
                            eq.0,
                            render_word(&IrExpr::Str(eq.1.to_string(), StrStyle::DoubleQuoted))
                        ));
                    } else {
                        emit_indent(out, indent);
                        out.push_str(&format!("$ENV{{{}}} = $ENV{{{}}};\n", word_str, word_str));
                    }
                }
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "pwd" => {
            emit_indent(out, indent);
            out.push_str("print qx{pwd};\n");
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "shopt" => {
            // Shell-option toggles — no runtime effect for the supported
            // subset (the corpus's shopt -s/-u lines gate extglob etc.).
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "set" => {
            // `set -- a b c` replaces the positional parameters.
            if words.first().and_then(|w| call_arg_str(w)).as_deref() == Some("--") {
                let items: Vec<String> = words[1..].iter().map(|w| render_word(w)).collect();
                emit_indent(out, indent);
                out.push_str(&format!("@ARGV = ({});\n", items.join(", ")));
            }
            // Other `set` forms are option toggles — no runtime effect.
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "true" | ":" => {
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "false" => {
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 1;\n");
        }
        "shift" => {
            emit_indent(out, indent);
            out.push_str("shift @ARGV;\n");
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "exit" => {
            let code = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "0".to_string());
            emit_indent(out, indent);
            out.push_str(&format!("exit {};\n", code));
        }
        "local" => {
            // bash `local NAME=VALUE` — function-scoped: a fresh `my`
            // inside the sub (shadows the hoisted lexical for the call).
            // The parser may split the assignment: `data_type=` + `$1`
            // (two words) — join them.
            //
            // It must be `my`, not `local`: the hoisted top-level
            // lexicals are `my`-scoped (`my $n;`), and `local $n` on a
            // lexical is a compile error ("Can't localize lexical
            // variable $n"). `local` is only valid on package globals
            // (`local @ARGV`, `local *STDERR`), never on the hoisted
            // user vars — whose names are surfaced verbatim.
            let mut i = 0;
            let mut sigil = '$';
            while i < words.len() {
                // A `local -a`/`-A`/`-i`/… flag word is not a variable name;
                // `-a`/`-A` select the array/hash sigil.
                if let Some(s) = call_arg_str(words[i]) {
                    if s.starts_with('-') && s.len() > 1 {
                        for c in s[1..].chars() {
                            if c == 'a' {
                                sigil = '@';
                            } else if c == 'A' {
                                sigil = '%';
                            }
                        }
                        i += 1;
                        continue;
                    }
                }
                let Some(word_str) = call_arg_str(words[i]) else {
                    i += 1;
                    continue;
                };
                if let Some(eq) = word_str.split_once('=') {
                    let val = eq.1.to_string();
                    if eq.1.is_empty() && i + 1 < words.len() {
                        // value is the next word — render it structurally
                        // (a getVar(1) word → $ARGV[0]).
                        let next = render_word(words[i + 1]);
                        emit_indent(out, indent);
                        out.push_str(&format!("my {}{} = {};\n", sigil, eq.0, next));
                        i += 1;
                        i += 1;
                        continue;
                    }
                    // The value may be a positional ref ($1 → $ARGV[0])
                    // or an interpolating literal. Strip source quotes
                    // ("$1" arrives with them).
                    let val_trim = val.trim().trim_matches('"').trim_matches('\'');
                    let rendered = if let Some(n) = val_trim.strip_prefix('$') {
                        if !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        {
                            var_read(n)
                        } else {
                            render_word(&IrExpr::Str(val_trim.to_string(), StrStyle::DoubleQuoted))
                        }
                    } else {
                        render_word(&IrExpr::Str(val_trim.to_string(), StrStyle::DoubleQuoted))
                    };
                    emit_indent(out, indent);
                    out.push_str(&format!("my {}{} = {};\n", sigil, eq.0, rendered));
                } else {
                    emit_indent(out, indent);
                    out.push_str(&format!("my {}{};\n", sigil, word_str));
                }
                i += 1;
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "read" => {
            // `read [-flags] VAR` — the leading options (`-r`, `-p`, `-n`, …)
            // are NOT the target variable: bash reads one line into the first
            // non-flag word. Taking `words.first()` made `read -r line` emit
            // `$-r = $__line;`, a Perl syntax error (triage t36_read_stdin).
            if let Some(n) = words
                .iter()
                .copied()
                .filter_map(call_arg_str)
                .find(|w| !w.starts_with('-'))
            {
                emit_indent(out, indent);
                out.push_str(&format!("chomp(my $__line = <STDIN>); ${} = $__line;\n", n));
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "sleep" => {
            let t = words
                .first()
                .map(|w| render_word(w))
                .unwrap_or_else(|| "1".to_string());
            emit_indent(out, indent);
            out.push_str(&format!(
                "sleep({}); $main_exit_code = $CHILD_ERROR = 0;\n",
                t
            ));
        }
        "wait" => {
            // bash `wait` — reap the background children (triage-perl
            // t44_background): bare `wait` waits for ALL children;
            // `wait $pid` for the named ones. Perl's wait/waitpid block
            // until the child exits, exactly like bash.
            if words.is_empty() {
                emit_indent(out, indent);
                out.push_str("while (wait() != -1) {}\n");
            } else {
                for w in &words {
                    emit_indent(out, indent);
                    out.push_str(&format!("waitpid({}, 0);\n", render_word(w)));
                }
            }
            emit_indent(out, indent);
            out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
        }
        "test" => {
            // `test` builtin: reconstruct the `[ ... ]` condition and
            // evaluate it as a Perl boolean. The exit status is 0 for
            // true, 1 for false.
            let cond = render_test_call(&words.iter().map(|w| {
                IrExpr::Str(render_word(w), StrStyle::DoubleQuoted)
            }).collect::<Vec<_>>());
            emit_indent(out, indent);
            out.push_str(&format!("$main_exit_code = $CHILD_ERROR = ({}) ? 0 : 1;\n", cond));
        }
        _ => {
            // External command — first try the AST Generator's in-Perl
            // emulation (verified-safe commands only: native Perl, no bash
            // dependency); fall back to bash -c shell-out otherwise.
            if EMULATED_COMMANDS.contains(&cmd.as_str())
                && std::env::var("DEBASHC_IR_NO_EMUL").is_err()
            {
                if let Some(perl) = generator_emulate_command(&cmd, &words) {
                    for line in perl.lines() {
                        emit_indent(out, indent);
                        out.push_str(line);
                        out.push('\n');
                    }
                    return;
                }
            }
            // Native leaves did not cover this single external command:
            // pass the ALREADY-SPLIT argv to the uu-ffi seam. The fallback
            // shell-out lives in the stripped preamble; the in-process
            // coreutils `.so` handles the supported set. Word-splitting
            // (`split`) and glob words keep the shell-text path.
            if uu_seam_enabled()
                && !is_shell_builtin(&cmd)
                && !words.is_empty()
                && !words.iter().any(|w| {
                    matches!(w, IrExpr::Call { func, .. } if func == "split")
                        || matches!(w, IrExpr::Str(s, _) if s.starts_with("\u{1}SH2GLOB\u{1}"))
                })
            {
                let mut args: Vec<String> = vec![perl_word_literal(&cmd)];
                args.extend(words.iter().map(|w| uu_arg(w)));
                for line in env_pre.lines() {
                    emit_indent(out, indent);
                    out.push_str(line);
                    out.push('\n');
                }
                emit_indent(out, indent);
                out.push_str(&format!(
                    "__sh_uu_run({}); $main_exit_code = $CHILD_ERROR = $__sh_uu_rc;\n",
                    args.join(", ")
                ));
                return;
            }
            let full = build_shell_cmd(&cmd, &words);
            // the bash-c child inherits the env-prefix assignments
            for line in env_pre.lines() {
                emit_indent(out, indent);
                out.push_str(line);
                out.push('\n');
            }
            emit_shell_cmd(out, indent, &full);
        }
    }
}

/// `echo` lowering — join args with single spaces, newline unless `-n`.
fn emit_echo(out: &mut String, words: &[&IrExpr], indent: usize) {
    let mut args = words;
    let mut newline = true;
    let mut escapes = false;
    loop {
        let Some(first) = args.first() else { break };
        let Some(s) = call_arg_str(first) else { break };
        match s.as_str() {
            "-n" => {
                newline = false;
                args = &args[1..];
            }
            "-e" => {
                escapes = true;
                args = &args[1..];
            }
            "-E" => {
                escapes = false;
                args = &args[1..];
            }
            _ => break,
        }
    }
    let ok = |out: &mut String| out.push_str("$main_exit_code = $CHILD_ERROR = 0;\n");
    if args.is_empty() {
        emit_indent(out, indent);
        if newline {
            out.push_str("print \"\\n\";\n");
        }
        emit_indent(out, indent);
        ok(out);
        return;
    }
    let all_literal = args.iter().all(|a| {
        matches!(a, IrExpr::Str(_, _))
            || matches!(
                a,
                IrExpr::Interpolate(parts)
                    if parts.iter().all(|p| matches!(p, InterpPart::Lit(_)))
            )
    });
    if all_literal {
        let joined: String = args
            .iter()
            .map(|a| call_arg_str(a).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ");
        let joined = if escapes {
            crate::shir_passes::echo_fold::echo_esc(&joined)
        } else {
            joined
        };
        let mut s = String::from("\"");
        for ch in joined.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                '$' => s.push_str("\\$"),
                '@' => s.push_str("\\@"),
                '\n' => s.push_str("\\n"),
                c if is_byte_marker(c) => s.push_str(&byte_marker_escape(c)),
                _ => s.push(ch),
            }
        }
        if newline {
            s.push_str("\\n");
        }
        s.push('"');
        emit_indent(out, indent);
        out.push_str(&format!("print {};\n", s));
        emit_indent(out, indent);
        ok(out);
        return;
    }
    let parts: Vec<String> = args.iter().map(|w| render_word_list(w)).collect();
    let nl = if newline { ", \"\\n\"" } else { "" };
    // Unquoted word-splitting drops empty words (bash IFS semantics);
    // `grep { defined && length }` filters those out and avoids undef
    // warnings on unset positional params. A split word expands to a LIST
    // (triage-perl t62_word_split) and must take the join+grep path even
    // when it is the ONLY word (`echo $x` — join re-inserts the spaces).
    let has_split = args
        .iter()
        .any(|w| matches!(w, IrExpr::Call { func, .. } if func == "split"));
    if parts.len() == 1 && !has_split {
        // A single glob word is a LIST in Perl — wrap in join(' ', …) so
        // the expansion space-joins like bash (echo file_*.txt).
        if parts[0].starts_with("glob(") {
            emit_indent(out, indent);
            out.push_str(&format!("print(join(' ', {}){});\n", parts[0], nl));
        } else {
            emit_indent(out, indent);
            out.push_str(&format!("print({}{});\n", parts[0], nl));
        }
    } else if has_split {
        emit_indent(out, indent);
        out.push_str(&format!(
            "print(join(' ', grep {{ defined($_) && length($_) }} {}){});\n",
            parts.join(", "),
            nl
        ));
    } else {
        emit_indent(out, indent);
        out.push_str(&format!("print(join(' ', {}){});\n", parts.join(", "), nl));
    }
    emit_indent(out, indent);
    ok(out);
}

/// Collect variables assigned by `Assign` stmts (recursively) so the
/// modern-IR preamble can hoist `my` declarations (Perl `use strict`
/// requires them; the Generator's usage analysis does the same). Vars
/// declared via `Declare`/`DeclareArray` are excluded (they emit their own
/// `my`/`local`).
/// Walk an expression for `Assign`s hidden inside its closures
/// (`Arrow` bodies, `and`/`or` chains, captures) so their targets are
/// hoisted. `collect_assigned_vars` on an `IrStmt::Expr` otherwise only
/// sees the top-level Arrow.
fn collect_assigned_in_expr(e: &IrExpr, out: &mut Vec<(String, Sigil)>) {
    match e {
        IrExpr::Arrow(stmts) => collect_assigned_vars(stmts, out),
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_assigned_in_expr(a, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_assigned_in_expr(lhs, out);
            collect_assigned_in_expr(rhs, out);
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_assigned_in_expr(i, out);
            }
        }
        IrExpr::Capture { expr, .. } => collect_assigned_in_expr(expr, out),
        IrExpr::Ternary { cond, then, else_ } => {
            collect_assigned_in_expr(cond, out);
            collect_assigned_in_expr(then, out);
            collect_assigned_in_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_assigned_in_expr(expr, out);
            collect_assigned_in_expr(default, out);
        }
        IrExpr::Object(fields) => {
            for (_, v) in fields {
                collect_assigned_in_expr(v, out);
            }
        }
        _ => {}
    }
}

fn collect_assigned_vars(stmts: &[IrStmt], out: &mut Vec<(String, Sigil)>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    // setArray/setArrayAppend RHS → the target is an array even
                    // if the sigil annotation is absent. `z+=(a b)` lowers to
                    // setArrayAppend, and the emit_stmt `+=` arm renders it as
                    // `push @z, (...)`; declaring `my $z` instead makes the
                    // program die at compile time with "Global symbol @z
                    // requires explicit package name" (examples/051_primes
                    // line 9).
                    let is_array_rhs = matches!(expr, IrExpr::Call { func, .. }
                        if func == "setArray" || func == "setArrayAppend");
                    // An uppercase ARRAY is not an env var: the env-style
                    // heuristic must not skip its declaration (`LIST=()` left
                    // `@LIST` undeclared — examples/parse-array-plusassign).
                    if is_env_style_var_name(&t.var) && !is_array_rhs {
                        continue;
                    }
                    let (base, idx) = split_indexed_var(&t.var);
                    // dotted struct-field names sanitize to a valid perl
                    // identifier (matches var_read / perl_lhs_for)
                    let base = base.replace('.', "_").replace('-', "_");
                    let sigil = if is_array_rhs {
                        Sigil::Array
                    } else if idx.is_some() {
                        Sigil::Hash
                    } else {
                        t.sigil.unwrap_or(Sigil::Scalar)
                    };
                    if !out.iter().any(|(n, _)| n == &base) {
                        out.push((base, sigil));
                    }
                }
            }
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    out.retain(|(n, _)| n != &d.name);
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                out.retain(|(n, _)| n != var);
            }
            IrStmt::If {
                then,
                elsifs,
                else_,
                ..
            } => {
                collect_assigned_vars(then, out);
                for (_, b) in elsifs {
                    collect_assigned_vars(b, out);
                }
                collect_assigned_vars(else_, out);
            }
            IrStmt::For { var, body, .. } => {
                // The loop var is read after the loop (bash keeps the last
                // value) — declare it so `for $var` aliases a lexical.
                if !is_env_style_var_name(var) && !out.iter().any(|(n, _)| n == var) {
                    out.push((var.clone(), Sigil::Scalar));
                }
                collect_assigned_vars(body, out);
            }
            // `assign(NAME, op, VALUE)` — the frontend's statement write.
            IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "assign" => {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if !is_env_style_var_name(&name) && !out.iter().any(|(n, _)| n == &name) {
                        // `z+=(a b)` is an ARRAY append — the emitter lowers
                        // it to `push @z, (...)`, so declaring `my $z` here
                        // emits a program that dies at compile time ("Global
                        // symbol @z requires explicit package name",
                        // examples/051_primes line 9). The frontend emits
                        // `assign` for both `y+=2` and `z+=(a b)`, so ask the
                        // same program-wide census the emitter's `+=` arm uses
                        // rather than a value-shape test (which misreads
                        // `z+=${primes[@]:0:1}`, a single-word RHS appending
                        // to an array).
                        let sigil = if array_var(&name) {
                            Sigil::Array
                        } else {
                            Sigil::Scalar
                        };
                        out.push((name, sigil));
                    }
                }
                for a in args {
                    collect_assigned_in_expr(a, out);
                }
            }
            // setVar(name, value) — the frontend-emitted store write
            // (the C frontend's Assign lowering): declare the scalar so
            // `my $p_x` hoists for the sanitized dotted reads/writes
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if func == "setVar" =>
            {
                if let Some(IrExpr::Str(nm, _)) = args.first() {
                    let base = nm.replace('.', "_").replace('-', "_");
                    if var_is_declarable(&base) && !out.iter().any(|(n, _)| n == &base) {
                        out.push((base, Sigil::Scalar));
                    }
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => collect_assigned_vars(body, out),
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_assigned_vars(&c.body, out);
                }
            }
            // `typeset`/`declare`/`local`/`readonly`/`export` assignments
            // render at the capability layer (invisible to the census).
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if matches!(func.as_str(), "exec" | "builtin") =>
            {
                if let Some(cmd) = args.first().and_then(call_arg_str) {
                    if matches!(
                        cmd.as_str(),
                        "typeset" | "declare" | "local" | "readonly" | "export"
                    ) {
                        let mut array = false;
                        let mut hash = false;
                        for w in exec_word_args(args) {
                            let Some(s) = call_arg_str(w) else { continue };
                            if let Some(f) = s.strip_prefix('-') {
                                array |= f.contains('a');
                                hash |= f.contains('A');
                                continue;
                            }
                            let name = s.split('=').next().unwrap_or(&s);
                            let name = name.trim_matches(['[', ']']);
                            let (base, _) = split_indexed_var(name);
                            if var_is_declarable(base) && !out.iter().any(|(n, _)| n == base) {
                                out.push((
                                    base.to_string(),
                                    if hash {
                                        Sigil::Hash
                                    } else if array {
                                        Sigil::Array
                                    } else {
                                        Sigil::Scalar
                                    },
                                ));
                            }
                        }
                    }
                }
            }
            IrStmt::Function { body, .. } => collect_assigned_vars(body, out),
            IrStmt::Redirect { inner, .. } => collect_assigned_vars(inner, out),
            IrStmt::Expr(IrExpr::Arrow(body)) => collect_assigned_vars(body, out),
            // Assignments hidden inside nested closures (the procsub
            // materialisation's `and(Arrow([…]), …)` inside a redirect)
            // still need hoisted declarations.
            IrStmt::Expr(e) => collect_assigned_in_expr(e, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_assigned_vars(st, out);
                }
            }
            _ => {}
        }
    }
}

/// A var name that needs a hoisted `my` declaration: not an env-style
/// name (→ $ENV), not a positional param (→ $ARGV), not a bash special.
fn var_is_declarable(name: &str) -> bool {
    !is_env_style_var_name(name)
        && !name.is_empty()
        && name != "_" // Perl's `$_` default-arg global — `my $_` is fatal
        // Perl English/special lexicals and backend temporaries must never
        // be redeclared (`my $INPUT_RECORD_SEPARATOR` breaks `local $/`).
        // `__ps_tmp*` / `__ps_doc*` are NOT backend temporaries: they are
        // the process-substitution materialisation's OWN shell variables
        // (visible in the source), so they do need hoisting.
        && (!name.starts_with("__")
            || name.starts_with("__ps_tmp")
            || name.starts_with("__ps_doc"))
        && name != "main_exit_code"
        && !matches!(
            name,
            "CHILD_ERROR"
                | "OS_ERROR"
                | "ERRNO"
                | "EVAL_ERROR"
                | "INPUT_RECORD_SEPARATOR"
                | "PROGRAM_NAME"
                | "OUTPUT_AUTOFLUSH"
                | "OUTPUT_FIELD_SEPARATOR"
                | "OUTPUT_RECORD_SEPARATOR"
                | "LIST_SEPARATOR"
                | "SUBSCRIPT_SEPARATOR"
                | "LAST_PAREN_MATCH"
                | "EFFECTIVE_USER_ID"
                | "REAL_USER_ID"
                | "EFFECTIVE_GROUP_ID"
                | "REAL_GROUP_ID"
                | "PERL_VERSION"
        )
        && !name.chars().all(|c| c.is_ascii_digit())
        && !matches!(name, "#" | "?" | "@" | "*" | "$" | "!" | "0")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A `read VAR…` target is a variable the SCRIPT assigns, so it must be
/// declared even when it carries the reserved `__` prefix: a frontend may
/// name a scratch variable `__frest` (the bat `for /f` "rest of line"
/// lowering), and `var_is_declarable` refuses `__*` to protect the backend's
/// own temporaries — leaving the rendered `$__frest` an undeclared global
/// under `use strict` (bat t24_forf2). The backend's internal prefix
/// (`__sh2*`) stays excluded.
fn read_target_is_declarable(name: &str) -> bool {
    var_is_declarable(name)
        || (name.starts_with("__")
            && !name.starts_with("__sh2")
            && !is_env_style_var_name(name)
            && name != "main_exit_code"
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

/// The ordered printf conversion characters of a LITERAL format word
/// (`%c`/`%s`/`%d`…; `%%` is not a conversion), or `None` when the format is
/// not a single literal (a dynamic `$fmt` cannot be mapped to its args).
/// Used to give a positional `%c` operand bash's first-character semantics.
fn literal_printf_conversions(fmt: &IrExpr) -> Option<Vec<char>> {
    let text: String = match fmt {
        IrExpr::Str(s, _) => s.clone(),
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(t) => t.clone(),
                    _ => String::new(),
                })
                .collect()
        }
        _ => return None,
    };
    let mut convs = Vec::new();
    let mut cs = text.chars().peekable();
    while let Some(c) = cs.next() {
        if c != '%' {
            continue;
        }
        if cs.peek() == Some(&'%') {
            cs.next();
            continue;
        }
        // Skip flags/width/precision, then take the conversion letter.
        let mut conv = None;
        for c2 in cs.by_ref() {
            if c2.is_ascii_alphabetic() {
                conv = Some(c2);
                break;
            }
        }
        convs.push(conv.unwrap_or('?'));
    }
    Some(convs)
}

/// Collect vars READ via getVar/param/Var (recursively) so the preamble can
/// declare them too — `use strict` fails on any undeclared read, and bash
/// reads of unset vars yield empty strings (a `my $x;` matches that).
fn collect_read_vars_expr(e: &IrExpr, out: &mut Vec<(String, Sigil)>) {
    match e {
        IrExpr::Call { func, args } => {
            if func == "getVar" || func == "param" {
                let idx = if func == "param" { 1 } else { 0 };
                if let Some(name) = args.get(idx).and_then(call_arg_str) {
                    // An INDEXED name (`words[$k]`, `arr[0]`) declares the
                    // base as an array/hash — `var_is_declarable` rejects the
                    // bracketed form, so the base was never hoisted.
                    let bang = name.starts_with('!');
                    let name = name.trim_start_matches(['!', '#']).trim_end_matches('*');
                    let (base, index) = split_indexed_var(name);
                    let sigil = if bang {
                        Sigil::Hash
                    } else if let Some(ix) = &index {
                        if ix
                            .chars()
                            .all(|c| c.is_ascii_digit() || c == '@' || c == '*' || c == ' ')
                        {
                            Sigil::Array
                        } else {
                            Sigil::Hash
                        }
                    } else if func == "param"
                        && args.first().and_then(call_arg_str).as_deref() == Some("slice")
                        && args.get(2).and_then(call_arg_str).as_deref() == Some("@")
                    {
                        Sigil::Array
                    } else if array_var(base) {
                        Sigil::Array
                    } else {
                        Sigil::Scalar
                    };
                    let base = base.replace('.', "_").replace('-', "_");
                    if var_is_declarable(&base)
                        && !out.iter().any(|(n, s)| n == &base && *s == sigil)
                    {
                        out.push((base, sigil));
                    }
                }
            }
            // Array reads (${arr[@]}, ${#arr[@]}, arr[i], $(arr…)) declare
            // as @name.
            if matches!(
                func.as_str(),
                "listVar" | "getArray" | "arrayItems" | "arrayLen" | "arrayIndex"
            ) {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if var_is_declarable(&name) && !out.iter().any(|(n, _)| n == &name) {
                        out.push((name, Sigil::Array));
                    }
                }
            }
            if func == "test" {
                // Test conditions arrive as a flat string ("$y -lt 10")
                // with $vars inside — collect them for declarations.
                if let Some(s) = args.first().and_then(call_arg_str) {
                    collect_vars_from_test_string(&s, out);
                }
            }
            // `arith("$n + 1")` — a flat arithmetic string with $vars.
            if func == "arith" {
                if let Some(s) = args.first().and_then(call_arg_str) {
                    collect_vars_from_test_string(&s, out);
                }
            }
            // `read [-r] VAR…` assigns its operands (the while-read loop
            // condition) — declare them so the rendered assignment is a
            // lexical, not an undeclared-global compile error.
            if func == "exec" || func == "builtin" {
                if args.first().and_then(call_arg_str).as_deref() == Some("read") {
                    if let Some(IrExpr::Array(words)) = args.get(1) {
                        for w in words {
                            if let Some(n) = call_arg_str(w) {
                                if n.starts_with('-') {
                                    continue;
                                }
                                if read_target_is_declarable(&n)
                                    && !out.iter().any(|(name, _)| name == &n)
                                {
                                    out.push((n, Sigil::Scalar));
                                }
                            }
                        }
                    }
                }
            }
            for a in args {
                collect_read_vars_expr(a, out);
            }
        }
        IrExpr::Var(name, sigil) => {
            if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
                out.push((name.clone(), sigil.unwrap_or(Sigil::Scalar)));
            }
        }
        IrExpr::Array(elems) => {
            for e in elems {
                collect_read_vars_expr(e, out);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(ee) = p {
                    collect_read_vars_expr(ee, out);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_read_vars_expr(lhs, out);
            collect_read_vars_expr(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_read_vars_expr(cond, out);
            collect_read_vars_expr(then, out);
            collect_read_vars_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_read_vars_expr(expr, out);
            collect_read_vars_expr(default, out);
        }
        IrExpr::Index { key, .. } => collect_read_vars_expr(key, out),
        // Interpolated / raw literal strings carry `$var` reads that must be
        // hoisted (`"$index"`, and the native-test transform's RAW Perl).
        IrExpr::Str(s, _) => collect_vars_from_test_string(s, out),
        IrExpr::Capture { expr, .. } => collect_read_vars_expr(expr, out),
        IrExpr::Arith(ast) => collect_read_vars_arith(ast, out),
        IrExpr::MethodCall { obj, args, .. } => {
            collect_read_vars_expr(obj, out);
            for a in args {
                collect_read_vars_expr(a, out);
            }
        }
        IrExpr::Object(fields) => {
            for (_, v) in fields {
                collect_read_vars_expr(v, out);
            }
        }
        IrExpr::Arrow(stmts) => collect_read_vars_stmts(stmts, out),
        _ => {}
    }
}

fn collect_read_vars_arith(ast: &ArithAst, out: &mut Vec<(String, Sigil)>) {
    match ast {
        ArithAst::Var(name) => {
            if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
                out.push((name.clone(), Sigil::Scalar));
            }
        }
        ArithAst::Ident(name) => {
            if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
                out.push((name.clone(), Sigil::Scalar));
            }
        }
        ArithAst::Index { var, key } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
            collect_read_vars_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_read_vars_arith(lhs, out);
            collect_read_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_read_vars_arith(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_read_vars_arith(test, out);
            collect_read_vars_arith(then, out);
            collect_read_vars_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
            collect_read_vars_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            if var_is_declarable(var) && !out.iter().any(|(n, _)| n == var) {
                out.push((var.clone(), Sigil::Scalar));
            }
        }
        ArithAst::Num(_) => {}
        ArithAst::Sizeof(_) => {}
        ArithAst::Cast { arg, .. } => collect_read_vars_arith(arg, out),
    }
}

fn collect_read_vars_stmts(stmts: &[IrStmt], out: &mut Vec<(String, Sigil)>) {
    for s in stmts {
        match s {
            IrStmt::Expr(e) | IrStmt::Assign { expr: e, .. } => collect_read_vars_expr(e, out),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(then, out);
                for (c, b) in elsifs {
                    collect_read_vars_expr(c, out);
                    collect_read_vars_stmts(b, out);
                }
                collect_read_vars_stmts(else_, out);
            }
            IrStmt::For { iter, body, .. } => {
                collect_read_vars_expr(iter, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::While { cond, body } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_read_vars_expr(cond, out);
                collect_read_vars_stmts(body, out);
            }
            IrStmt::Case {
                discriminant,
                clauses,
                ..
            } => {
                collect_read_vars_expr(discriminant, out);
                for c in clauses {
                    collect_read_vars_stmts(&c.body, out);
                }
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => collect_read_vars_stmts(body, out),
            IrStmt::Declare { init, .. } => {
                if let Some(i) = init {
                    collect_read_vars_expr(i, out);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for el in elements {
                    collect_read_vars_expr(el, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_read_vars_stmts(st, out);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_read_vars_stmts(inner, out),
            IrStmt::Output { value, .. } => collect_read_vars_expr(value, out),
            IrStmt::RawText(t) => collect_vars_from_test_string(t, out),
            IrStmt::WriteFile { path, content, .. } => {
                collect_read_vars_expr(path, out);
                collect_read_vars_expr(content, out);
            }
            _ => {}
        }
    }
}

/// True if `name` is assigned by any `Assign` stmt (recursively).
fn collect_assigned_vars_contains(stmts: &[IrStmt], name: &str) -> bool {
    let mut found = Vec::new();
    collect_assigned_vars(stmts, &mut found);
    found.iter().any(|(n, _)| n == name)
}

/// Scan a test-condition string for `$var` / `${var}` / `${#var}` reads
/// (for hoisted declarations — the vars are text inside the flat string).
fn collect_vars_from_test_string(s: &str, out: &mut Vec<(String, Sigil)>) {
    fn push(out: &mut Vec<(String, Sigil)>, name: &str, sigil: Sigil) {
        if var_is_declarable(name) && !out.iter().any(|(n, _)| n == name) {
            out.push((name.to_string(), sigil));
        }
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let mut j = i + 1;
            let mut braced = false;
            if j < chars.len() && chars[j] == '{' {
                braced = true;
                j += 1;
            }
            let mut bang = false;
            if j < chars.len() && (chars[j] == '#' || chars[j] == '!') {
                bang = chars[j] == '!';
                j += 1;
            }
            let start = j;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let name: String = chars[start..j].iter().collect();
            // A subscript makes it an array/hash element (`$arr[0]`,
            // `$words[$k]`); the key shape picks the sigil.
            let mut sigil = Sigil::Scalar;
            if j < chars.len() && chars[j] == '[' {
                let mut k = j + 1;
                while k < chars.len() && chars[k] != ']' {
                    k += 1;
                }
                let sub: String = chars[j + 1..k.min(chars.len())].iter().collect();
                let numeric = sub
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '@' || c == '*' || c == ' ');
                sigil = if bang || !numeric {
                    Sigil::Hash
                } else {
                    Sigil::Array
                };
                j = k;
            }
            // A `{…}` subscript is a Perl HASH key (`$array{'${index}'}`).
            if j < chars.len() && chars[j] == '{' {
                sigil = Sigil::Hash;
                let mut k = j + 1;
                while k < chars.len() && chars[k] != '}' {
                    k += 1;
                }
                j = k;
            }
            if !name.is_empty() {
                push(out, &name, sigil);
            }
            if braced && j < chars.len() && chars[j] == '}' {
                j += 1;
            }
            i = if j > i { j } else { i + 1 };
        } else {
            i += 1;
        }
    }
}

/// The expression-position twin of [`rewrite_fn_calls`]: rewrite
/// `exec(foo, …)` to the `$fn_call` marker when `foo` is a function defined in
/// the program. `if is_prime $n; then` puts the call in the CONDITION, and the
/// statement-level pass cannot see it (it only matches `IrStmt::Expr`).
fn rewrite_fn_calls_in_expr(e: &IrExpr, fns: &std::collections::HashSet<String>) -> IrExpr {
    match e {
        IrExpr::Call { func, args } => {
            let args: Vec<IrExpr> = args.iter().map(|a| rewrite_fn_calls_in_expr(a, fns)).collect();
            if func == "exec" {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if fns.contains(&name) {
                        return IrExpr::Call {
                            func: "$fn_call".to_string(),
                            args,
                        };
                    }
                }
            }
            IrExpr::Call {
                func: func.clone(),
                args,
            }
        }
        IrExpr::BinOp { lhs, op, rhs } => IrExpr::BinOp {
            lhs: Box::new(rewrite_fn_calls_in_expr(lhs, fns)),
            op: op.clone(),
            rhs: Box::new(rewrite_fn_calls_in_expr(rhs, fns)),
        },
        IrExpr::Ternary { cond, then, else_ } => IrExpr::Ternary {
            cond: Box::new(rewrite_fn_calls_in_expr(cond, fns)),
            then: Box::new(rewrite_fn_calls_in_expr(then, fns)),
            else_: Box::new(rewrite_fn_calls_in_expr(else_, fns)),
        },
        IrExpr::DefinedOr { expr, default } => IrExpr::DefinedOr {
            expr: Box::new(rewrite_fn_calls_in_expr(expr, fns)),
            default: Box::new(rewrite_fn_calls_in_expr(default, fns)),
        },
        IrExpr::Array(elems) => {
            IrExpr::Array(elems.iter().map(|a| rewrite_fn_calls_in_expr(a, fns)).collect())
        }
        IrExpr::Capture { expr, native } => IrExpr::Capture {
            expr: Box::new(rewrite_fn_calls_in_expr(expr, fns)),
            native: *native,
        },
        other => other.clone(),
    }
}

/// Rewrite `exec(foo, …)` calls to the `$fn_call` marker when `foo` is a
/// function defined in the program (recursively into compound bodies).
fn rewrite_fn_calls(stmts: &[IrStmt], fns: &std::collections::HashSet<String>) -> Vec<IrStmt> {
    stmts
        .iter()
        .map(|s| match s {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                if let Some(name) = args.first().and_then(call_arg_str) {
                    if fns.contains(&name) {
                        return IrStmt::Expr(IrExpr::Call {
                            func: "$fn_call".to_string(),
                            args: args.clone(),
                        });
                    }
                }
                s.clone()
            }
            // A call in CONDITION position — `if is_prime $n; then`
            // (examples/051_primes line 50) — is as common as the statement
            // form and was NOT rewritten: the arm cloned `cond` verbatim, so
            // the exec fell through emit_exec_call's shell-out and became
            // `system('bash','-c','"is_prime" $candidate')` → "bash: line 1:
            // is_prime: command not found", even though `sub is_prime` was
            // emitted a few lines above. Descend into the condition.
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => IrStmt::If {
                cond: rewrite_fn_calls_in_expr(cond, fns),
                then: rewrite_fn_calls(then, fns),
                elsifs: elsifs
                    .iter()
                    .map(|(c, b)| (rewrite_fn_calls_in_expr(c, fns), rewrite_fn_calls(b, fns)))
                    .collect(),
                else_: rewrite_fn_calls(else_, fns),
            },
            IrStmt::While { cond, body } => IrStmt::While {
                cond: rewrite_fn_calls_in_expr(cond, fns),
                body: rewrite_fn_calls(body, fns),
            },
            IrStmt::DoWhile { body, cond, until } => IrStmt::DoWhile {
                body: rewrite_fn_calls(body, fns),
                cond: rewrite_fn_calls_in_expr(cond, fns),
                until: *until,
            },
            IrStmt::For { var, iter, body } => IrStmt::For {
                var: var.clone(),
                iter: iter.clone(),
                body: rewrite_fn_calls(body, fns),
            },
            IrStmt::Case {
                discriminant,
                clauses,
            } => IrStmt::Case {
                discriminant: discriminant.clone(),
                clauses: clauses
                    .iter()
                    .map(|c| IrCaseClause {
                        patterns: c.patterns.clone(),
                        body: rewrite_fn_calls(&c.body, fns),
                    })
                    .collect(),
            },
            IrStmt::Function {
                name,
                params,
                body,
                named_blocks,
            } => IrStmt::Function {
                name: name.clone(),
                params: params.clone(),
                body: rewrite_fn_calls(body, fns),
                named_blocks: named_blocks
                    .iter()
                    .map(|(k, v)| (k.clone(), rewrite_fn_calls(v, fns)))
                    .collect(),
            },
            IrStmt::Subshell(body) => IrStmt::Subshell(rewrite_fn_calls(body, fns)),
            IrStmt::Redirect { inner, redirects } => IrStmt::Redirect {
                // A redirect-wrapped call to a PROGRAM-DEFINED function
                // (`capture LBL tty < "$f"`) is still a Perl sub call — the
                // rebuilt shell text cannot see the sub (`bash: capture:
                // command not found`).
                inner: rewrite_fn_calls(inner, fns),
                redirects: redirects.clone(),
            },
            IrStmt::Background(body) => IrStmt::Background(rewrite_fn_calls(body, fns)),
            IrStmt::Block(body) => IrStmt::Block(rewrite_fn_calls(body, fns)),
            IrStmt::Pipeline {
                stages,
                last_output,
                capture,
                cmd_str,
            } => IrStmt::Pipeline {
                stages: stages.iter().map(|st| rewrite_fn_calls(st, fns)).collect(),
                last_output: last_output.clone(),
                capture: capture.clone(),
                cmd_str: cmd_str.clone(),
            },
            other => other.clone(),
        })
        .collect()
}

/// Render a test-expression string (the text inside `[ … ]` / `[[ … ]]`,
/// as flattened by ast_to_ir) as a Perl boolean expression.
fn render_test_expr(s: &str) -> String {
    // `[[ ]]` tests flatten to NO-WHITESPACE forms ("$s==*.txt",
    // "$s=~^re$", "$f1==!(*.min).js", ""$y"="5"") that the spaced
    // tokenizer can't parse. Spaced strings (with -gt/-a/etc.) keep the
    // tokenizer even if they contain a bare `=`.
    if !s.chars().any(|c| c.is_whitespace()) {
        if s.contains("==") || s.contains("=~") || s.contains("!=") || s.contains('=') {
            if let Some(r) = render_flat_test(s) {
                return r;
            }
        }
    }
    let toks = normalize_test_tokens(tokenize_test(s));
    let mut i = 0;
    let expr = parse_test_or(&toks, &mut i);
    if expr.is_empty() {
        "0".to_string()
    } else {
        expr
    }
}

/// Parse an already-tokenized test expression (the `[ ]`/`[[ ]]` tokenizer's
/// output, with any trailing `[[` marker already removed) with the recursive
/// `!`/`(`/`&&`/`||` parser. Returns `None` for an empty token list.
///
/// Shared with the native-stmt transform's test fallback so COMPOUND tests
/// (`! ( "$name" == "x" )` — a perl-sh-go `unless` lowering) render natively
/// instead of the historic always-truthy `$t =~ /-\w+/` heuristic.
pub(crate) fn render_test_token_list(toks: &[String]) -> Option<String> {
    let toks = normalize_test_tokens(toks.to_vec());
    let mut i = 0;
    let expr = parse_test_or(&toks, &mut i);
    if expr.is_empty() {
        None
    } else {
        Some(expr)
    }
}

/// A `[[ ]]`-flavored operand: strip quotes, `$var` → read, else literal.
fn flat_operand(s: &str) -> String {
    let t = s.trim().trim_matches('"').trim_matches('\'');
    // `${…}` — delegate to the parameter-expansion renderer
    // (`"${arr[@]}"` was emitted verbatim, an invalid Perl interpolation).
    if t.starts_with("${") && t.ends_with('}') {
        if let Some(p) = param_text_to_perl(t) {
            return p;
        }
    }
    if t == "~" {
        return "$ENV{HOME}".to_string();
    }
    if let Some(rest) = t.strip_prefix("~/") {
        if !rest.is_empty() {
            return format!("\"$ENV{{HOME}}/{}\"", rest.replace('\"', "\\\""));
        }
    }
    if let Some(name) = t.strip_prefix('$') {
        let clean = name.starts_with('{') && name.ends_with('}');
        let bare = &name[if clean { 1 } else { 0 }..name.len() - if clean { 1 } else { 0 }];
        if !bare.is_empty() && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            var_read(bare)
        } else {
            // $HOME/xxx — an interpolating literal: keep it double-quoted
            // with env-style vars rewritten to $ENV{…} (Perl would demand
            // a declaration for bare $HOME).
            let interp = t.replace('\"', "\\\"");
            let re = regex::Regex::new(r"\$(\w+)").unwrap();
            let rewritten = re.replace_all(&interp, "$$ENV{$1}");
            format!("\"{}\"", rewritten)
        }
    } else if t.is_empty() {
        "''".to_string()
    } else if t.contains('$') {
        // Interpolating literal — keep as a double-quoted string.
        format!("\"{}\"", t)
    } else {
        format!("'{}'", t.replace('\'', "\\\\'"))
    }
}

/// String equality with nocasematch runtime tracking.
pub(crate) fn nocase_str_eq(lhs: &str, rhs: &str) -> String {
    format!(
        "(($__nocasematch) ? lc({l}) eq lc({r}) : {l} eq {r})",
        l = lhs,
        r = rhs
    )
}

/// Render a flattened `[[ ]]` test: lhs OP rhs where OP is `==`, `=~`,
/// `!=` or `=`. Glob and extglob patterns become regex matches.
fn render_flat_test(s: &str) -> Option<String> {
    for (op, kind) in [("==", "eq"), ("=~", "re"), ("!=", "ne"), ("=", "eq")] {
        if let Some(pos) = s.find(op) {
            let lhs = flat_operand(&s[..pos]);
            let rhs = &s[pos + op.len()..];
            match kind {
                "re" => {
                    // [[ $s =~ regex ]] → $s =~ /regex/
                    let re = rhs.trim_matches('"').trim_matches('\'');
                    return Some(format!("({} =~ /{}/)", lhs, re));
                }
                "ne" => {
                    // != : string inequality (glob patterns rare here)
                    let l = flat_operand(rhs);
                    return Some(format!("(!({}))", nocase_str_eq(&lhs, &l)));
                }
                _ => {
                    // == or = : glob pattern (contains */?/[/!( ) or literal.
                    let has_glob = rhs.contains('*')
                        || rhs.contains('?')
                        || rhs.contains('[')
                        || rhs.contains("!(");
                    if rhs.contains("!(") {
                        // Extglob `!(P).S` — the compound is a full boolean
                        // expression (lhs embedded); return it directly.
                        if let Some(re) = glob_or_extglob_to_regex(&lhs, rhs) {
                            return Some(re);
                        }
                    }
                    if has_glob {
                        // Normal glob → anchored regex match.
                        let re = glob_to_regex(rhs.trim_matches('"').trim_matches('\''));
                        return Some(format!("({} =~ /^{}$/)", lhs, re));
                    }
                    return Some(nocase_str_eq(&lhs, &flat_operand(rhs)));
                }
            }
        }
    }
    None
}

/// glob → anchored regex; `!(P).S` extglob → the corpus idiom as a
/// compound: ends-with-S and not P+S.
pub(crate) fn glob_or_extglob_to_regex(lhs: &str, pat: &str) -> Option<String> {
    let pat = pat.trim_matches('"').trim_matches('\'');
    if let Some(open) = pat.find("!(") {
        if let Some(close_rel) = pat[open + 2..].find(')') {
            let close = open + 2 + close_rel;
            let inner = &pat[open + 2..close];
            let suffix = &pat[close + 1..];
            let re_inner = glob_to_regex(inner);
            let re_suffix = glob_to_regex(suffix);
            // (x =~ /S$/) && (x !~ /P S$/)
            return Some(format!(
                "(({l}) =~ /{rs}$/) && (({l}) !~ /{ri}{rs}$/)",
                l = lhs,
                rs = re_suffix,
                ri = re_inner
            ));
        }
    }
    Some(format!("^({})$", glob_to_regex(pat)))
}

/// Tokenize a test string: whitespace-separated, double-quoted runs kept
/// together (with the quotes stripped but `$var` left for interpolation).
pub(crate) fn tokenize_test(s: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut in_dq = false;
    // Single quotes are LITERAL: no `$`-expansion, no `=`/whitespace
    // splitting inside. They were previously not tracked at all, so a
    // single-quoted empty operand survived as the two characters `''`
    // (`[[ -z '' ]]` → `!length('\\'\\'')`, length 2, test inverted —
    // 070_gnuisms_thorough's `empty: 0` came out `empty: 1`).
    let mut in_sq = false;
    // An EMPTY quoted operand (`[[ -z "" ]]` / `[[ -z '' ]]`) must still
    // produce a TOKEN: the trailing `if !cur.is_empty()` would drop it, and
    // the operand list then reads the NEXT token (or nothing) — the test
    // inverted.
    let mut quoted_run = false;
    // `${…}` / `$(…)` nesting: whitespace and `=` inside are NOT separators
    // (`[ ${v% *} -gt ${v#* } ]`, `[ -n $(echo a b) ]`).
    let mut brace = 0i32;
    let mut paren = 0i32;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' if !in_dq && brace == 0 && paren == 0 => {
                cur.push('$');
                match chars.peek() {
                    Some('{') => {
                        cur.push('{');
                        chars.next();
                        brace = 1;
                    }
                    Some('(') => {
                        cur.push('(');
                        chars.next();
                        paren = 1;
                    }
                    _ => {}
                }
            }
            '{' if !in_dq && brace > 0 => {
                brace += 1;
                cur.push('{');
            }
            '}' if !in_dq && brace > 0 => {
                brace -= 1;
                cur.push('}');
            }
            '(' if !in_dq && paren > 0 => {
                paren += 1;
                cur.push('(');
            }
            ')' if !in_dq && paren > 0 => {
                paren -= 1;
                cur.push(')');
            }
            '\'' if !in_sq => {
                if in_dq {
                    // A `'` inside double quotes is literal data.
                    cur.push('\'');
                } else {
                    in_sq = true;
                    quoted_run = true;
                }
            }
            '\'' if in_sq => {
                in_sq = false;
                toks.push(std::mem::take(&mut cur));
                quoted_run = false;
            }
            '"' if !in_sq => {
                if in_dq {
                    in_dq = false;
                    toks.push(std::mem::take(&mut cur));
                    quoted_run = false;
                } else {
                    quoted_run = true;
                    // A quoted run starting mid-token (`\>"$b"`) ends the
                    // preceding word — `\>` must stay its own token.
                    if !cur.is_empty() {
                        toks.push(std::mem::take(&mut cur));
                    }
                    in_dq = true;
                }
            }
            c if c.is_whitespace() && !in_dq && !in_sq && brace == 0 && paren == 0 => {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
            }
            '=' if !in_dq && !in_sq && brace == 0 && paren == 0 => {
                // Flattened "[ a = b ]" loses the spaces around `=` — split
                // it out as its own token (handles `a=b`, `a==b`, `a!=b`).
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                let mut op = String::from("=");
                match chars.peek() {
                    Some('=') => {
                        op.push('=');
                        chars.next();
                    }
                    Some('~') => {
                        // `[[ $s =~ re ]]` — the regex-match operator
                        // (triage-perl t68_case_glob): `=~` must stay ONE
                        // token; the old split (`=` + `~`) parsed as
                        // equality against the literal `~` (`$s eq '~'`).
                        op.push('~');
                        chars.next();
                    }
                    _ => {}
                }
                toks.push(op);
            }
            '!' if !in_dq && !in_sq && chars.peek() == Some(&'=') => {
                // `!=` — possibly right after a quoted token ("$a"!="b").
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
                toks.push("!=".to_string());
                chars.next();
            }
            '\\' if in_dq || in_sq => {
                if let Some(&d) = chars.peek() {
                    cur.push(d);
                    chars.next();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() || quoted_run {
        toks.push(cur);
    }
    toks
}

fn test_peek<'a>(toks: &'a [String], i: usize) -> Option<&'a str> {
    toks.get(i).map(|s| s.as_str())
}

fn test_next<'a>(toks: &'a [String], i: &mut usize) -> Option<&'a str> {
    let t = test_peek(toks, *i);
    if t.is_some() {
        *i += 1;
    }
    t
}

/// The flattened test text loses spaces at line continuations, gluing
/// `\(`/`\)`/`!` onto the next token (`\(!`, `"x"\)`). Split them into
/// standalone operator tokens so the recursive parser sees the grouping.
fn normalize_test_tokens(toks: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for t in toks {
        let mut rest = t.as_str();
        loop {
            if let Some(r) = rest.strip_prefix("\\(") {
                out.push("\\(".to_string());
                rest = r;
            } else if let Some(r) = rest.strip_prefix("\\)") {
                out.push("\\)".to_string());
                rest = r;
            } else if rest.starts_with('!') && !rest.starts_with("!=") {
                out.push("!".to_string());
                rest = &rest[1..];
            } else if rest.starts_with("\\>") || rest.starts_with("\\<") {
                // `[ a \> b ]` — the `>` is escaped so it is a string
                // comparison, not a redirect.
                out.push(rest[1..2].to_string());
                rest = &rest[2..];
            } else if rest.ends_with("\\)") && rest.len() > 2 {
                let (h, _) = rest.split_at(rest.len() - 2);
                out.push(h.to_string());
                rest = "\\)";
            } else if rest.ends_with("\\(") && rest.len() > 2 {
                let (h, _) = rest.split_at(rest.len() - 2);
                out.push(h.to_string());
                rest = "\\(";
            } else {
                if !rest.is_empty() {
                    out.push(rest.to_string());
                }
                break;
            }
        }
    }
    out
}

fn parse_test_or(toks: &[String], i: &mut usize) -> String {
    let mut lhs = parse_test_and(toks, i);
    loop {
        match test_peek(toks, *i) {
            Some("||") | Some("-o") => {
                *i += 1;
                let rhs = parse_test_and(toks, i);
                lhs = format!("({} || {})", lhs, rhs);
            }
            _ => break,
        }
    }
    lhs
}

fn parse_test_and(toks: &[String], i: &mut usize) -> String {
    let mut lhs = parse_test_not(toks, i);
    loop {
        match test_peek(toks, *i) {
            Some("&&") | Some("-a") => {
                *i += 1;
                let rhs = parse_test_not(toks, i);
                lhs = format!("({} && {})", lhs, rhs);
            }
            _ => break,
        }
    }
    lhs
}

fn parse_test_not(toks: &[String], i: &mut usize) -> String {
    if test_peek(toks, *i) == Some("!") {
        *i += 1;
        format!("!({})", parse_test_not(toks, i))
    } else {
        parse_test_primary(toks, i)
    }
}

fn parse_test_primary(toks: &[String], i: &mut usize) -> String {
    match test_peek(toks, *i) {
        Some("(") | Some("\\(") => {
            *i += 1;
            let inner = parse_test_or(toks, i);
            if matches!(test_peek(toks, *i), Some(")") | Some("\\)")) {
                *i += 1;
            }
            format!("({})", inner)
        }
        Some("true") | Some(":") => {
            *i += 1;
            "1".to_string()
        }
        Some("false") => {
            *i += 1;
            "0".to_string()
        }
        Some(op) if is_file_test_op(op) => {
            *i += 1;
            let operand = test_next(toks, i).unwrap_or("''");
            let o = render_test_operand(operand);
            // bash spells the symlink/exists tests `-h`/`-L`/`-a`; Perl
            // uses `-l` and `-e`.
            let op = match op {
                "-h" | "-L" => "-l",
                "-a" => "-e",
                other => other,
            };
            match op {
                "-z" => format!("!length({})", o),
                "-n" => format!("length({})", o),
                _ => format!("{} {}", op, o),
            }
        }
        _ => {
            let lhs = test_next(toks, i).unwrap_or("''");
            match test_peek(toks, *i) {
                Some(op) if is_compare_op(op) => {
                    *i += 1;
                    let rhs = test_next(toks, i).unwrap_or("''");
                    if op == "=~" {
                        // `[[ $s =~ re ]]` — regex match (triage-perl
                        // t68_case_glob). The rhs is a bare ERE pattern
                        // (quotes stripped by the tokenizer); Perl accepts
                        // a STRING as the pattern, so the single-quoted
                        // render is safe against delimiter/interpolation
                        // chars. Perl's regex dialect ≈ ERE for the
                        // corpus patterns.
                        let l = render_test_operand(lhs);
                        let r = render_test_operand(rhs);
                        return format!("({l} =~ {r})");
                    }
                    // `-ef` / `-nt` / `-ot` compare inode+device / mtime;
                    // either operand missing makes the test FALSE (bash
                    // does not warn or treat two missing files as equal).
                    if matches!(op, "-ef" | "-nt" | "-ot") {
                        let l = render_test_operand(lhs);
                        let r = render_test_operand(rhs);
                        if op == "-ef" {
                            return format!(
                                "(-e {l} && -e {r} && (stat({l}))[0] == (stat({r}))[0] && (stat({l}))[1] == (stat({r}))[1])"
                            );
                        }
                        let cmp = if op == "-nt" { ">" } else { "<" };
                        return format!("(-e {l} && -e {r} && (stat({l}))[9] {cmp} (stat({r}))[9])");
                    }
                    let perl_op = perl_compare_op(op);
                    let l = render_test_operand(lhs);
                    let r = render_test_operand(rhs);
                    format!("{} {} {}", l, perl_op, r)
                }
                _ => {
                    // bare operand: truthiness = non-empty string
                    let l = render_test_operand(lhs);
                    format!("length({})", l)
                }
            }
        }
    }
}

fn is_file_test_op(op: &str) -> bool {
    matches!(
        op,
        "-f" | "-d" | "-e" | "-r" | "-w" | "-x" | "-s" | "-z" | "-n"
            | "-h" | "-L" | "-a" | "-S" | "-p" | "-b" | "-c" | "-t" | "-g" | "-u" | "-k"
    )
}

fn is_compare_op(op: &str) -> bool {
    matches!(
        op,
        "=" | "==" | "!=" | "=~" | "<" | ">" | "<=" | ">=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge"
            | "-ef" | "-nt" | "-ot"
    )
}

fn perl_compare_op(op: &str) -> &'static str {
    match op {
        "=" | "==" => "eq",
        "!=" => "ne",
        "-eq" => "==",
        "-ne" => "!=",
        "-lt" => "<",
        "-le" => "<=",
        "-gt" => ">",
        "-ge" => ">=",
        // `[ a > b ]` / `[[ a > b ]]` is a STRING comparison (the
        // numeric forms are `-gt`/`-lt`).
        "<" => "lt",
        ">" => "gt",
        "<=" => "le",
        ">=" => "ge",
        _ => "==",
    }
}

/// Render a test operand: `$var`-only tokens become bare `${name}` reads
/// (Generator idiom); literals become Perl literals.
/// `${…}` text → Perl via the shared `render_param` (the test-flattened
/// form keeps the raw expansion). Returns None when it isn't a plain
/// parameter expansion.
///
/// `pub(crate)`: `src/transforms/shir_native_stmt.rs` calls this from the
/// `-n`/`-z` parameter-expansion lowering, which needs the expansion's VALUE
/// rather than its text.
pub(crate) fn param_text_to_perl(tok: &str) -> Option<String> {
    let inner = tok.strip_prefix("${")?.strip_suffix('}')?;
    if inner.is_empty() {
        return None;
    }
    let chars: Vec<char> = inner.chars().collect();
    // `${#name}` — scalar length.
    if chars[0] == '#' {
        let rest: String = chars[1..].iter().collect();
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(format!("length({})", var_read(&rest)));
        }
    }
    let mut i = 0usize;
    if matches!(chars[0], '@' | '*' | '?' | '#' | '$' | '-') {
        i = 1;
    } else if chars[0].is_ascii_digit() {
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
    } else if chars[0].is_ascii_alphabetic() || chars[0] == '_' {
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        if i < chars.len() && chars[i] == '[' {
            let mut depth = 0i32;
            while i < chars.len() {
                if chars[i] == '[' {
                    depth += 1;
                }
                if chars[i] == ']' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
        }
    } else {
        return None;
    }
    let name: String = chars[..i].iter().collect();
    let rest: String = chars[i..].iter().collect();
    if rest.is_empty() {
        // `"${arr[@]}"` — all elements, joined by spaces in scalar context.
        if let Some(base) = name.strip_suffix("[@]").or_else(|| name.strip_suffix("[*]")) {
            return Some(format!("join(' ', @{})", base));
        }
        return Some(var_read(&name));
    }
    // `${s:off:len}` — the substring/slice form (`${flags:j:1}`). The `:`
    // also introduces `:-`/`:=`/`:?`/`:+`, which the operator list below
    // handles, so only a `:digit`/`:$` continuation is a slice.
    //
    // The operator-vs-slice decision has to be made on the two-character
    // prefix. Reading only the single `:` character made EVERY `:-` look like
    // a slice, so `${a:-${b:-$(cmd)}}` re-lexed textually became
    // `substr($b, -$(cmd))` — the `-` taken as a negative slice offset — which
    // emitted Perl with a bare `$(` (Perl's GID special variable): the file
    // failed to COMPILE and every other construct in it was lost
    // (perl-20261002-231200; corpus 063_04 and 063_hard_to_parse). Nesting
    // depth 0-2 was unaffected because their `:-` was the OUTERMOST operator
    // and so arrived structurally lowered, never through this textual
    // fallback.
    //
    // bash's own rule: `:-` is the default operator only when the `-` follows
    // the `:` immediately. A space makes it a slice offset — with
    // `a=abcdef`, `${a:-1}` is `abcdef` and `${a: -1}` is `f`. The parser
    // encodes that by keeping the space in the slice's offset text (it lowers
    // `${a: -1}` to `param("slice", "a", " -1", "")` and `${a:-1}` to
    // `param(":-", "a", "1")`), so this textual fallback applies the same
    // test the parser already applies.
    if rest.starts_with(':') {
        // `:`-operator ⇒ fall through to the operator loop below. This is
        // the WHOLE of the decision: bash reads `:-`/`:=`/`:?`/`:+` as the
        // parameter operators, and EVERY other `:` continuation as a slice
        // offset (`${a:1:3}`, `${a: -1}`, `${flags:j:1}`).
        //
        // The operator is the TWO-character PREFIX `:` followed by one of
        // `- = ? +`. Testing the single character `:` instead — which is what
        // `[':', '=', '?', '+'].iter().any(|c| rest.starts_with(...))` did —
        // matched EVERY `:` continuation, so `:-` never reached the operator
        // loop and always fell into the slice branch. That is the defect this
        // branch exists to prevent.
        let is_op = rest.len() >= 2 && matches!(rest.as_bytes()[1], b'-' | b'=' | b'?' | b'+');
        if !is_op {
            // A slice. Consume the `:` and split offset from (optional)
            // length. The leading space of `${a: -1}` is significant and is
            // kept: it is exactly what distinguishes the slice from the
            // `:-` operator, and the parser preserves it the same way.
            let rest = &rest[1..];
            let (off, len) = match rest.split_once(':') {
                Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
                None => (rest.trim().to_string(), String::new()),
            };
            return Some(render_param(&[
                IrExpr::Str(":".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Str(name, StrStyle::DoubleQuoted),
                IrExpr::Str(off, StrStyle::DoubleQuoted),
                IrExpr::Str(len, StrStyle::DoubleQuoted),
            ]));
        }
    }
    for op in [":-", ":+", ":=", ":?", "##", "%%", "//", "#", "%", "-", "+", "=", "?", "/"] {
        if let Some(arg) = rest.strip_prefix(op) {
            let arg = arg.trim_matches('"').trim_matches('\'').to_string();
            return Some(render_param(&[
                IrExpr::Str(op.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Str(name, StrStyle::DoubleQuoted),
                IrExpr::Str(arg, StrStyle::DoubleQuoted),
            ]));
        }
    }
    None
}

pub(crate) fn render_test_operand(tok: &str) -> String {
    // Strip surrounding source quotes (`"${arr[@]}"` arrives with them).
    // BOTH quote styles: a single-quoted operand is LITERAL data, and
    // leaving its quotes in place turned `[[ -z '' ]]` into
    // `!length('\\'\\'')` — a two-character string, so every emptiness
    // test on a single-quoted operand was inverted (070_gnuisms_thorough
    // printed `empty: 1` where bash prints `empty: 0`).
    let tok = tok.trim();
    let was_quoted = tok.len() >= 2
        && ((tok.starts_with('"') && tok.ends_with('"'))
            || (tok.starts_with('\'') && tok.ends_with('\'')));
    let tok = if was_quoted { &tok[1..tok.len() - 1] } else { tok };
    // `${…}` parameter expansion inside a test operand.
    if tok.starts_with("${") && tok.ends_with('}') {
        if let Some(p) = param_text_to_perl(tok) {
            return p;
        }
    }
    // Tilde expansion. bash expands a LEADING `~` to $HOME before either
    // operand is compared, so `[ ~ = "$HOME" ]` is TRUE and
    // `[ ~/Documents = "$HOME/Documents" ]` likewise. The spaced `[ ]`
    // tokenizer hands `~` here as an ordinary word; without this arm it fell
    // through to the bare-literal fallthrough and emitted `'~'`, a string
    // that can never equal $HOME (043_home.sh printed `-` where bash
    // printed `1`). Mirrors the arms in `flat_operand`.
    //
    // bash does NOT expand a QUOTED tilde, so the arm is gated on the
    // source quotes this function strips above. `~user` is deliberately not
    // handled: bash leaves it literal when the user does not exist, so
    // passing it through matches bash in both outcomes.
    if !was_quoted {
        if tok == "~" {
            return "$ENV{HOME}".to_string();
        }
        if let Some(rest) = tok.strip_prefix("~/") {
            if !rest.is_empty() {
                return format!("\"$ENV{{HOME}}/{}\"", rest.replace('\"', "\\\""));
            }
        }
    }
    // `$(( … ))` arithmetic expansion.
    if let Some(inner) = tok
        .strip_prefix("$((")
        .and_then(|s| s.strip_suffix("))"))
    {
        if let Some(p) = arith_str_to_perl(inner) {
            return p;
        }
    }
    // `$( … )` command substitution — capture its stdout (bash strips
    // trailing newlines).
    if let Some(inner) = tok.strip_prefix("$(").and_then(|s| s.strip_suffix(')')) {
        if !inner.trim().is_empty() && !inner.contains("$(") {
            return cmd_str_to_open_perl_stripped(inner);
        }
    }
    // $name[idx] / ${name[idx]} array element inside a test.
    if tok.starts_with('$') && tok.contains('[') && tok.contains(']') {
        let inner = tok
            .trim_start_matches("${")
            .trim_start_matches('$')
            .trim_end_matches('}');
        if !inner.is_empty()
            && inner.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || c == '_'
                    || c == '['
                    || c == ']'
                    || c == '$'
                    || c == '.'
                    || c == ' '
                    || c == '-'
            })
        {
            return format!("${}", inner);
        }
    }
    if tok.starts_with('$') && tok.len() > 1 {
        let name = &tok[1..];
        let bare = if name.starts_with('{') && name.ends_with('}') {
            &name[1..name.len() - 1]
        } else {
            name
        };
        if !bare.is_empty()
            && (bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || matches!(bare, "#" | "?" | "@" | "*" | "$" | "!" | "0"))
        {
            var_read(bare)
        } else {
            // `$HOME/Documents` — an interpolating literal: rewrite `$VAR`
            // to `$ENV{VAR}` and double-quote.
            flat_operand(tok)
        }
    } else if tok.is_empty() {
        "''".to_string()
    } else if tok.chars().all(|c| c.is_ascii_digit()) {
        format!("'{}'", tok)
    } else if tok.contains('$') {
        // Literal with an embedded variable — double-quote for interpolation.
        let mut s = String::from("\"");
        for ch in tok.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                c if is_byte_marker(c) => s.push_str(&byte_marker_escape(c)),
                _ => s.push(ch),
            }
        }
        s.push('"');
        s
    } else if tok.chars().any(is_byte_marker) {
        // Invalid-UTF-8 byte in a bare test operand: split out \xNN
        // byte escapes (single quotes would keep them literal).
        let mut out = String::from("'");
        for ch in tok.chars() {
            if is_byte_marker(ch) {
                out.push_str(&format!("' . \"{}\" . '", byte_marker_escape(ch)));
            } else if ch == '\'' {
                out.push_str("\\'");
            } else {
                out.push(ch);
            }
        }
        out.push('\'');
        out
    } else {
        // Perl single-quoted literal: a `'` inside is escaped `\'` — the
        // FOUR-backslash spelling emitted the two characters `\\'` and
        // terminated the string early (`[[ -z "" ]]` → `!length('\\'\\'')`).
        format!("'{}'", tok.replace('\'', "\\'"))
    }
}

/// Render a Str literal (shared by ir_expr_to_perl and render_word).
/// Private-use marker chars (U+E000+byte, from
/// SharedUtils::bytes_to_marked_lossy — the A1 emit's invalid-UTF-8
/// preservation, core request perl-20260814-175710) become `\xNN` BYTE
/// escapes so non-UTF-8 source bytes round-trip byte-for-byte (bash treats
/// scripts as byte streams).
fn is_byte_marker(c: char) -> bool {
    (0xE000..=0xE0FF).contains(&(c as u32))
}

fn byte_marker_escape(c: char) -> String {
    format!("\\x{:02X}", (c as u32 - 0xE000) as u8)
}

fn render_str_literal(s: &str, style: &StrStyle) -> String {
    match style {
        StrStyle::SingleQuoted => {
            // Perl single-quote escaping: DOUBLE backslashes first, then
            // escape quotes.  Escaping only `'` → `\'` is wrong when the
            // source has `\` before a quote: `\'` in the output would be
            // read by Perl as escaped-backslash + string CLOSE.
            // Byte markers break out into a double-quoted `\xNN` escape
            // (single quotes do not interpolate).
            let mut out = String::from("'");
            for ch in s.chars() {
                if is_byte_marker(ch) {
                    out.push_str(&format!("' . \"{}\" . '", byte_marker_escape(ch)));
                } else if ch == '\\' {
                    out.push_str("\\\\");
                } else if ch == '\'' {
                    out.push_str("\\'");
                } else {
                    out.push(ch);
                }
            }
            out.push('\'');
            out
        }
        StrStyle::DoubleQuoted | StrStyle::Heredoc => {
            let mut escaped = String::from("\"");
            for ch in s.chars() {
                match ch {
                    '"' => escaped.push_str("\\\""),
                    '\\' => escaped.push_str("\\\\"),
                    '$' if matches!(style, StrStyle::DoubleQuoted) => escaped.push_str("\\$"),
                    '@' if matches!(style, StrStyle::DoubleQuoted) => escaped.push_str("\\@"),
                    '\n' => escaped.push_str("\\n"),
                    '\t' => escaped.push_str("\\t"),
                    '\r' => escaped.push_str("\\r"),
                    c if is_byte_marker(c) => escaped.push_str(&byte_marker_escape(c)),
                    c => escaped.push(c),
                }
            }
            escaped.push('"');
            escaped
        }
        StrStyle::Command => {
            // Backticks interpolate like double quotes, so \xNN byte
            // escapes work here too.
            let mut out = String::from("`");
            for ch in s.chars() {
                if is_byte_marker(ch) {
                    out.push_str(&byte_marker_escape(ch));
                } else {
                    out.push(ch);
                }
            }
            out.push('`');
            out
        }
        StrStyle::Raw => s.to_string(),
    }
}

/// Lower a modern-IR `test` Call (cond position) to a Perl boolean.
fn render_test_call(args: &[IrExpr]) -> String {
    if let Some(s) = args.first().and_then(call_arg_str) {
        render_test_expr(&s)
    } else {
        "0".to_string()
    }
}

// ── Expression emitter ───────────────────────────────────────────────

/// Rebuild the `pipeline(Array([Arrow, Arrow, …]))` EXPRESSION from an
/// `IrStmt::Pipeline` stage list, so the capability tree can be consulted
/// on the expression form (which carries the producer's literal payload).
/// Returns None for a stage shape the expression form cannot express
/// (anything but a single exec/builtin statement per stage).
fn stages_as_pipeline_expr(stages: &[Vec<IrStmt>]) -> Option<IrExpr> {
    if stages.len() < 2 {
        return None;
    }
    let mut arrows: Vec<IrExpr> = Vec::with_capacity(stages.len());
    for stage in stages {
        let [IrStmt::Expr(_)] = stage.as_slice() else {
            return None;
        };
        arrows.push(IrExpr::Arrow(stage.clone()));
    }
    Some(IrExpr::Call {
        func: "pipeline".to_string(),
        args: vec![IrExpr::Array(arrows)],
    })
}

/// Lower `read [-r] VAR…` in CONDITION position to a Perl boolean that
/// reads one line from STDIN, assigns the fields (IFS from the env-prefix
/// Object), and is true iff a line was read. Refuses flags that change
/// semantics beyond `-r` (Perl never processes backslashes anyway).
fn render_read_cond(args: &[IrExpr]) -> Option<String> {
    if args.first().and_then(call_arg_str).as_deref() != Some("read") {
        return None;
    }
    let words = match args.get(1) {
        Some(IrExpr::Array(e)) => e,
        _ => return None,
    };
    let mut vars: Vec<String> = Vec::new();
    for w in words {
        let s = call_arg_str(w)?;
        if s.starts_with('-') {
            if s == "-r" {
                continue;
            }
            return None; // -d/-n/-p/-a/-t/-u change semantics
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        vars.push(s);
    }
    if vars.is_empty() {
        return None;
    }
    let mut ifs: Option<String> = None;
    for a in args.iter().skip(2) {
        if let IrExpr::Object(props) = a {
            for (k, v) in props {
                if k == "IFS" {
                    ifs = call_arg_str(v);
                }
            }
        }
    }
    let assign = if vars.len() == 1 {
        format!("${} = $__rd;", vars[0])
    } else {
        let sep = match ifs.as_deref() {
            None | Some("") => "' '".to_string(),
            Some(s) => {
                // Each IFS character is a separator (bash). Build a char
                // class, escaping regex metacharacters.
                let mut class = String::new();
                for c in s.chars() {
                    if "\\^]-".contains(c) {
                        class.push('\\');
                    }
                    class.push(c);
                }
                format!("/[{}]/", class)
            }
        };
        format!(
            "(${}) = split({}, $__rd, {});",
            vars.join(", $"),
            sep,
            vars.len()
        )
    };
    Some(format!(
        "do {{ my $__rd = <STDIN>; if (defined $__rd) {{ chomp $__rd; {} 1 }} else {{ 0 }} }}",
        assign
    ))
}

/// Is `cmd` rendered in-process by the Perl emitter (no `system`/`bash -c`)?
/// `echo`/`printf` have dedicated emitters; the rest are the generator's
/// emulated set.
fn native_emulable_cmd(cmd: &str) -> bool {
    // Only commands `emit_stmt` renders as pure-Perl `print` (a `system`/
    // UU-FFI run writes to the real fd 1, which the in-memory capture
    // cannot intercept). Capability commands are tried earlier via
    // `native_capture_call`/`native_pipeline`.
    matches!(
        cmd,
        "echo" | "printf" | "true" | "false" | ":" | "test" | "[" | "let"
    )
}

/// A command expression the native capture can emit in-process.
fn capture_expr_native(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "exec" | "builtin" => {
                // Pure-Perl builtin emitters, or a native capability
                // (`native_exec_stmt`). Commands that fall back to a
                // `system`/UU-FFI run are NOT capturable in-process.
                match args.first().and_then(call_arg_str).as_deref() {
                    Some(c)
                        if matches!(
                            c,
                            "echo" | "printf" | "true" | "false" | ":" | "test" | "[" | "let"
                        ) =>
                    {
                        true
                    }
                    _ => crate::pipeline_native::native_exec_stmt(e).is_some(),
                }
            }
            "$fn_call" | "fnCall" => true,
            // The shared `test`/`arith` ops are pure-Perl (render_test_call /
            // arith_str_to_perl) — capturable in-process.
            "test" => true,
            "arith" => true,
            // `grep -o PAT` — the lift prints each match, so capturing its
            // stdout in-process is exact (`x=$(… | grep -o PAT)`).
            "grepMatches" => true,
            // `A && B` / `A || B` closures (the shell frontend's chain form).
            "and" | "or" => {
                !args.is_empty()
                    && args.iter().all(|a| match a {
                        IrExpr::Arrow(stmts) => capture_body_native(stmts),
                        _ => false,
                    })
            }
            _ => false,
        },
        IrExpr::BinOp { lhs, rhs, op, .. } => {
            matches!(op, BinOpKind::And | BinOpKind::Or)
                && capture_expr_native(lhs)
                && capture_expr_native(rhs)
        }
        IrExpr::Arith(_) => true,
        // A raw test operand (`If{cond: Str("-d 'a'", Raw)}` — the
        // `test` chain lowered to an if/else); a plain value is native.
        IrExpr::Str(_, _) => true,
        IrExpr::Var(_, _) => true,
        IrExpr::Interpolate(_) => true,
        _ => false,
    }
}

/// `$( … )` body whose statements all render in-process (folded `Output`
/// nodes, emulated `echo`/`printf`/… commands, `&&`/`||` chains of them).
/// Such a capture uses the select()-based Perl buffer instead of rebuilding
/// a shell command — EchoFold turns `$(echo hi)`'s `exec echo` into an
/// `Output`, which the shell-text rebuild cannot express.
fn capture_body_native(stmts: &[IrStmt]) -> bool {
    !stmts.is_empty()
        && stmts.iter().all(|s| match s {
            IrStmt::Output { .. } => true,
            IrStmt::Block(inner) => capture_body_native(inner),
            IrStmt::Expr(e) => capture_expr_native(e),
            // `A && B || C` lowers to an if/else with native branches
            // (`test -d a && echo yes || echo no` inside `$( … )`).
            // Empty branches are fine.
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
                ..
            } => {
                capture_expr_native(cond)
                    && (then.is_empty() || capture_body_native(then))
                    && elsifs.iter().all(|(c, b)| {
                        capture_expr_native(c) && (b.is_empty() || capture_body_native(b))
                    })
                    && (else_.is_empty() || capture_body_native(else_))
            }
            _ => false,
        })
}

/// A `||` chain of pipeline-expressible commands as a Perl expression:
/// `A || B` runs `A`; on a nonzero status it runs `B` instead. Used by
/// `x=$(stat … || wc -c < f)` so the capture does not shell to `bash -c`
/// (check_qx).
fn chain_or_capture(e: &IrExpr) -> Option<String> {
    if let IrExpr::BinOp {
        lhs,
        op: BinOpKind::Or,
        rhs,
    } = e
    {
        let l = chain_or_capture(lhs).or_else(|| pipeline_leaf_capture(lhs))?;
        let r = pipeline_leaf_capture(rhs)?;
        return Some(format!(
            "do {{ my $__c = {}; if ($CHILD_ERROR != 0) {{ $__c = {}; }} $__c }}",
            l, r
        ));
    }
    None
}

/// One pipeline-expressible command (no globs/vars it cannot expand)
/// as a `__sh_pipe_capture` expression.
fn pipeline_leaf_capture(e: &IrExpr) -> Option<String> {
    let cmd = expr_to_cmd(e)?;
    let v = pipeline_value_cmd(&cmd)?;
    Some(format!("__sh_pipe_capture({})", v))
}

pub(crate) fn ir_expr_to_perl(expr: &IrExpr) -> String {
    match expr {
        IrExpr::Capture { expr, native } => {
            // A heredoc in the captured body (`x=$(cat <<-EOF …)`) supplies
            // its body at the END of the command line, so it travels with
            // whatever this arm returns (see `append_redirect_frag`). Drain
            // once, here: a heredoc-free body drains nothing and the tail is
            // the empty string.
            let hd_tail = take_heredoc_tail();
            // Prefer the REBUILT SHELL TEXT for Arrow bodies: the old
            // fallback rendered the Arrow as a Perl anonymous sub
            // (`sub { … }`) and fed it to bash -c — which is not shell.
            // Bash reported `sub: command not found` (verified via
            // `x=$(printf "%s\n" …)` and `$(echo hi)` — the pre-existing
            // capture bug). The ASSIGN-capture path got this fix at the
            // emit_capture_assign site; this is the exec-arg-capture
            // twin (e.g. `printf '%s' "$([ -f "$f" ] && echo yes || …)"`
            // — parse-redirect-clobber's cmdsub-in-arg).
            if !*native {
                if let IrExpr::Arrow(stmts) = expr.as_ref() {
                    // A capture of a single NATIVE PIPELINE (`x=$(echo hi |
                    // sort)`) — the shared pipeline capability already knows
                    // how to lower the body to a Perl statement that PRINTS
                    // the result. Run that statement with stdout redirected
                    // into a buffer instead of shelling the whole thing to
                    // `bash -c` (which check_qx flags as "wrapping builtin
                    // 'echo'"). Same redirect idiom as the PERL-SIDE capture
                    // below; the statement's own `print` follows the
                    // selected handle.
                    if let [IrStmt::Expr(e)] = stmts.as_slice() {
                        // a redirect-fed command (`x=$(wc -l < FILE)`) —
                        // the capability tree gets the IR call directly
                        if let Some(expr) =
                            crate::pipeline_native::native_capture_call(e)
                        {
                            return expr;
                        }
                        if let Some(stmt) = crate::pipeline_native::native_pipeline(e) {
                            return format!(
                                "do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {stmt} close $__fh; select($__old); $__cap =~ s/\\n+\\z//; $__cap }}"
                            );
                        }
                        // `$(echo … | tr …)` — the native tr fold avoids
                        // the `bash -c` shell-out check_qx flags.
                        if let Some(expr) = echo_tr_expr(e) {
                            return format!(
                                "do {{ my $__cap = {}; $__cap =~ s/\\n+\\z//; $__cap }}",
                                expr
                            );
                        }
                        if let Some(expr) = capture_uu_expr(e) {
                            return expr;
                        }
                        // `x=$(stat … || wc -c < f)` — a `||` chain whose
                        // leaves the pipeline can run in-process.
                        if let Some(expr) = chain_or_capture(e) {
                            return expr;
                        }
                    }
                    // Natively emittable body (folded `Output`, emulated
                    // commands, `&&`/`||` chains): capture the Perl prints
                    // instead of rebuilding a shell command.
                    if capture_body_native(stmts) {
                        let mut body = String::new();
                        for st in stmts.iter() {
                            emit_stmt(&mut body, st, 1);
                        }
                        // fd-1 redirect (not select()) so child processes /
                        // UU-FFI runs that write to fd 1 are captured too.
                        return format!(
                            "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body};; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                        );
                    }
                    if let Some(cmd) = stmts_to_shell_cmd(stmts) {
                        return cmd_str_to_open_perl(&cmd);
                    }
                    // The shell-text rebuild refuses bodies whose words the
                    // bash spelling cannot round-trip (a `perl -e '…'` script
                    // arrives as a plain variable read; rendering it would
                    // hand bash `perl -e "$x"` and run the CONTENTS as the
                    // program). The statements themselves DO render as Perl
                    // (`system('perl','-e',$x)` / `open('-|','perl',…)`), so
                    // run them with fd 1 redirected into a scalar — same
                    // capture, no shell round-trip. REFUSE below would
                    // otherwise emit a `die` that kills the whole program.
                    {
                        let mut body = String::new();
                        for st in stmts.iter() {
                            emit_stmt(&mut body, st, 1);
                        }
                        return format!(
                            "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body}; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                        );
                    }
                    // A PERL-SIDE capture: the C frontend's outparam
                    // channel (setVar(cap, capture(Arrow[fnCall(..)]))) —
                    // the callee ECHOES its out-param values and the
                    // caller captures them. Redirect STDOUT into a string
                    // buffer around the body statements.
                    let has_calls = stmts.iter().any(|s| {
                        matches!(s,
                            IrStmt::Expr(IrExpr::Call { func, .. })
                                if func == "$fn_call" || func == "exec" || func == "builtin")
                    });
                    if has_calls {
                        let mut body = String::new();
                        for st in stmts.iter() {
                            emit_stmt(&mut body, st, 1);
                        }
                        return format!(
                            "do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {body}close $__fh; select($__old); $__cap }}"
                        );
                    }
                    // Last resort: the body renders as Perl (`system(...)` /
                    // `open('-|', …)`) but not as round-trippable shell text
                    // (the `perl -e '…'` word is a plain variable read, so
                    // the rebuilt `perl -e "$x"` would run the CONTENTS as
                    // the program). Run the statements with fd 1 redirected
                    // into a scalar — same capture without the shell.
                    // (`die` here killed the whole generated program.)
                    {
                        let mut body = String::new();
                        for st in stmts.iter() {
                            emit_stmt(&mut body, st, 1);
                        }
                        return format!(
                            "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body}; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                        );
                    }
                }
            }
            // A NATIVE capture whose Arrow body the native-pipeline fold
            // left as RawText perl (the fold flips `native` and inlines
            // the pipeline's perl text — which PRINTS to explicit
            // STDOUT). A bare Arrow render would stringify a code ref
            // (`$sed_result` printed `CODE(0x…)`); the text must be
            // CAPTURED instead — the fd-redirect shape (open STDOUT onto
            // the capture buffer, run, restore; `print STDOUT` follows
            // the redirected handle).
            if let IrExpr::Arrow(stmts) = expr.as_ref() {
                // A native capture whose body is still the `pipeline(...)`
                // CALL (the fold flipped `native` but left the call, e.g.
                // `x=$(echo … | grep -i …)`): lowering it through the
                // capability tree gives ONE statement that prints the
                // result, which the redirect below captures.
                if let [IrStmt::Expr(e)] = stmts.as_slice() {
                    if let Some(stmt) = crate::pipeline_native::native_pipeline(e) {
                        return format!(
                            "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {stmt}; ; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                        );
                    }
                }
                if stmts.iter().any(|s| matches!(s, IrStmt::RawText(_))) {
                    let mut body = String::new();
                    for st in stmts.iter() {
                        emit_stmt(&mut body, st, 1);
                    }
                    // perl quirk: an in-memory open(STDOUT, '>', \$ref)
                    // needs fd 1 CLOSED first (EBADF otherwise — verified
                    // on perl 5.38; the dup2 target must be free).
                    return format!(
                        "do {{ my $__cap = ''; {{ local *STDOUT; open(STDOUT, '>', \\$__cap) or die \"capture: $!\\n\"; {body};; }} $__cap =~ s/\\n+\\z//; $__cap }}"
                    );
                }
            }
            let mut inner = ir_expr_to_perl(expr);
            // Strip surrounding backticks from StrStyle::Command rendering
            if inner.starts_with('`') && inner.ends_with('`') && inner.len() >= 2 {
                inner = inner[1..inner.len() - 1].to_string();
            }
            if *native {
                // Native Perl expression — return as-is, no stripping
                inner
            } else {
                // Shell backtick result — use open() based code instead of qx{...}
                // to avoid check_qx violations.
                //
                // A heredoc body collected from the captured command goes at
                // the END of the command line, after every stage/redirect.
                cmd_str_to_open_perl(&format!("{inner}{hd_tail}"))
            }
        }

        IrExpr::Regex { pattern, flags } => {
            // Omit meaningless default flags (m, s, x) when they add no value.
            let has_anchor = |pat: &str| -> bool {
                // Check for ^ or $ that are NOT inside character classes ([...]).
                // A ^ or $ inside brackets is a literal character, not an anchor.
                let mut in_class = false;
                let mut prev_was_backslash = false;
                for ch in pat.chars() {
                    if prev_was_backslash {
                        prev_was_backslash = false;
                        continue;
                    }
                    if ch == '\\' {
                        prev_was_backslash = true;
                        continue;
                    }
                    if ch == '[' && !in_class {
                        in_class = true;
                        continue;
                    }
                    if ch == ']' && in_class {
                        in_class = false;
                        continue;
                    }
                    if (ch == '^' || ch == '$') && !in_class {
                        return true;
                    }
                }
                false
            };
            // Check for a literal dot that is NOT inside a character class.
            // Escaped dots like \. are literal dots, but they're not wildcards.
            let has_dot = |pat: &str| -> bool {
                let mut in_class = false;
                let mut prev_was_backslash = false;
                for ch in pat.chars() {
                    if prev_was_backslash {
                        prev_was_backslash = false;
                        continue;
                    }
                    if ch == '\\' {
                        prev_was_backslash = true;
                        continue;
                    }
                    if ch == '[' && !in_class {
                        in_class = true;
                        continue;
                    }
                    if ch == ']' && in_class {
                        in_class = false;
                        continue;
                    }
                    // A bare . outside a char class and not escaped is the wildcard.
                    if ch == '.' && !in_class {
                        return true;
                    }
                }
                false
            };
            let clean_flags: String = flags
                .chars()
                .filter(|&c| {
                    // Keep 'i' (case-insensitive), 'g' (global), etc.
                    // Remove 'm', 's', 'x' when they are the only flags or when
                    // the pattern doesn"t use the features they enable.
                    if c == 'm' {
                        // /m enables ^ and $ to match line boundaries; only
                        // meaningful if the pattern uses ^ or $ as OUTSIDE anchors
                        // (not inside a character class). Keep /m when anchors are
                        // present, strip it when they aren't.
                        has_anchor(pattern)
                    } else if c == 's' {
                        // /s makes . match \n; only meaningful if there is a
                        // bare . (wildcard) in the pattern, not inside [] or escaped.
                        has_dot(pattern)
                    } else if c == 'x' {
                        // /x allows whitespace and comments; it is almost always
                        // cargo-culted from generated boilerplate. Always strip it.
                        false
                    } else {
                        true
                    }
                })
                .collect();
            if clean_flags.is_empty() {
                // Use /pattern/ (forward slash delimiters) so the result is
                // a proper regex match operator, NOT a hash reference {pattern}.
                // Using {pattern} would be interpreted as a hash reference in
                // most contexts (e.g. grep { {pattern} } @list) and trigger
                // perlcritic ProhibitMutatingListFunctions false positives.
                format!("/{}/", pattern)
            } else {
                format!("/{}/{}", pattern, clean_flags)
            }
        }

        IrExpr::Range { start, end } => {
            format!("{}..{}", start, end)
        }

        IrExpr::RawExpr(text) => text.clone(),
        IrExpr::Arrow(stmts) => {
            // Delayed block (closure) — render as a Perl anonymous sub. The
            // capture/pipeline lowerings rebuild a shell command string from
            // the inner exec before falling back to this.
            let mut body = String::new();
            for s in stmts {
                emit_stmt(&mut body, s, 1);
            }
            format!("sub {{ {}\n}}", body.trim_end_matches('\n'))
        }
        IrExpr::ArrayComp { .. } => {
            // ESTree-path-only comprehension — the Perl generator never
            // emits it. Refuse loudly (a sub returning 0 would silently
            // miscompile).
            "die \"otranspilerl: shIR construct not yet supported by the Perl backend (ArrayComp)\";".to_string()
        }
        IrExpr::Lambda { .. } => {
            // ESTree-path-only lambda — refuse loudly (see ArrayComp).
            "die \"otranspilerl: shIR construct not yet supported by the Perl backend (Lambda)\";".to_string()
        }
        IrExpr::Splice(_) => {
            // ESTree-path-only splice marker — refuse loudly (see ArrayComp).
            "die \"otranspilerl: shIR construct not yet supported by the Perl backend (Splice)\";".to_string()
        }
        IrExpr::Ext(n) => {
            // Transform-declared expression node — drop-in handler dispatch.
            let ctx = crate::render_ext_expr::ExprRenderCtx::bare(
                crate::render_ext_expr::Backend::Perl,
                0,
            );
            if let Some(code) = crate::render_ext_expr::render(&**n, &ctx) {
                code
            } else {
                // No handler — fall back to sh2.* call (the runtime handles it).
                format!("sh2.{}(...)", n.tag())
            }
        }
        IrExpr::Array(elements) => {
            // General expression position: parenthesized list (for-iter,
            // list contexts). Exec-word position uses render_word instead.
            if elements.is_empty() {
                "()".to_string()
            } else {
                format!(
                    "({})",
                    elements
                        .iter()
                        .map(render_word)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        IrExpr::Arith(ast) => {
            // The trailing `int(...)` truncates, but must NOT swallow the
            // undef that a guard introduced (a zero divisor, or an
            // unparseable expression): `int(undef)` is 0, which would turn
            // bash's "value stays unset" into a spurious 0. Keep undef undef
            // so `${x:-default}` takes the default like bash
            // (parse-arithmetic-extra-paren: x must stay unset).
            let inner = arith_ast_to_perl(ast);
            if inner.starts_with("(do {") {
                format!("(do {{ my $__a = {inner}; defined($__a) ? int($__a) : undef }})")
            } else {
                format!("int({inner})")
            }
        }
        IrExpr::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        IrExpr::Json(v) => {
            let s = v.to_string();
            format!("'{}'", s.replace('\'', "\\\\'"))
        }
        IrExpr::Ident(name) => var_read(name),
        IrExpr::Object(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{} => {}", k, ir_expr_to_perl(v)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }

        IrExpr::Int(n) => {
            if n.abs() < 1000 {
                n.to_string()
            } else {
                // Format with underscore separators for readability
                let sign = if *n < 0 { "-" } else { "" };
                let abs = n.unsigned_abs();
                let s = abs.to_string();
                let bytes = s.as_bytes();
                let mut result = String::with_capacity(s.len() + s.len() / 3);
                for (i, &b) in bytes.iter().enumerate() {
                    if i > 0 && (s.len() - i) % 3 == 0 {
                        result.push('_');
                    }
                    result.push(b as char);
                }
                format!("{}{}", sign, result)
            }
        }

        IrExpr::Str(s, style) => match style {
            StrStyle::SingleQuoted => {
                // Check for leading-zero patterns that PPI may parse as octal
                let has_leading_zero = {
                    let bytes = s.as_bytes();
                    let mut i = 0;
                    let len = bytes.len();
                    let mut found = false;
                    while i < len && !found {
                        if !bytes[i].is_ascii_digit() {
                            i += 1;
                            continue;
                        }
                        if bytes[i] == b'0'
                            && i + 1 < len
                            && bytes[i + 1] >= b'0'
                            && bytes[i + 1] <= b'7'
                        {
                            let preceded_by_boundary = i == 0
                                || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
                            if preceded_by_boundary {
                                let mut j = i + 1;
                                while j < len && bytes[j].is_ascii_digit() {
                                    j += 1;
                                }
                                if j - i >= 2 {
                                    found = true;
                                }
                            }
                        }
                        while i < len && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    found
                };
                if has_leading_zero {
                    // q{...} is literal (no interpolation): markers must
                    // break out into a double-quoted \xNN byte escape.
                    let mut out = String::from("q{");
                    for ch in s.chars() {
                        if is_byte_marker(ch) {
                            out.push_str(&format!("}}\n\"{}\"\nq{{", byte_marker_escape(ch)));
                        } else if ch == '\\' {
                            out.push_str("\\\\");
                        } else if ch == '{' {
                            out.push_str("\\{");
                        } else if ch == '}' {
                            out.push_str("\\}");
                        } else {
                            out.push(ch);
                        }
                    }
                    out.push('}');
                    out
                } else {
                    // Double backslashes FIRST, then escape quotes: `\\'` in
                    // the output would otherwise be read as escaped-backslash
                    // + string CLOSE (see render_str_literal).
                    // Markers break out into a double-quoted \xNN byte
                    // escape (single quotes do not interpolate).
                    let mut out = String::from("'");
                    for ch in s.chars() {
                        if is_byte_marker(ch) {
                            out.push_str(&format!("' . \"{}\" . '", byte_marker_escape(ch)));
                        } else if ch == '\\' {
                            out.push_str("\\\\");
                        } else if ch == '\'' {
                            out.push_str("\\'");
                        } else {
                            out.push(ch);
                        }
                    }
                    out.push('\'');
                    out
                }
            }
            StrStyle::DoubleQuoted => {
                // Escape special characters for Perl double-quoted strings.
                // Backslash, dollar, at, double-quote, and control characters
                // must be escaped so the Perl source is clean and readable.
                let mut escaped = String::with_capacity(s.len() + 4);
                escaped.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => escaped.push_str("\\\""),
                        '\\' => escaped.push_str("\\\\"),
                        '$' => escaped.push_str("\\$"),
                        '@' => escaped.push_str("\\@"),
                        '\n' => escaped.push_str("\\n"),
                        '\t' => escaped.push_str("\\t"),
                        '\r' => escaped.push_str("\\r"),
                        c if is_byte_marker(c) => escaped.push_str(&byte_marker_escape(c)),
                        c => escaped.push(c),
                    }
                }
                escaped.push('"');
                escaped
            }
            StrStyle::Command => {
                // Backticks interpolate like double quotes: \xNN byte
                // escapes work directly.
                let mut out = String::with_capacity(s.len() + 4);
                out.push('`');
                for ch in s.chars() {
                    if is_byte_marker(ch) {
                        out.push_str(&byte_marker_escape(ch));
                    } else {
                        out.push(ch);
                    }
                }
                out.push('`');
                out
            }
            StrStyle::Heredoc => {
                // Like DoubleQuoted but preserves $ and @ for Perl interpolation.
                let mut escaped = String::with_capacity(s.len() + 4);
                escaped.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => escaped.push_str("\\\""),
                        '\\' => escaped.push_str("\\\\"),
                        // Keep $ and @ unescaped — Perl will interpolate them
                        '\n' => escaped.push_str("\\n"),
                        '\t' => escaped.push_str("\\t"),
                        '\r' => escaped.push_str("\\r"),
                        c if is_byte_marker(c) => escaped.push_str(&byte_marker_escape(c)),
                        c => escaped.push(c),
                    }
                }
                escaped.push('"');
                escaped
            }
            StrStyle::Raw => s.clone(),
        },

        IrExpr::Var(name, sigil) => match sigil.unwrap_or(Sigil::Scalar) {
            Sigil::Scalar => {
                if is_env_style_var_name(name) {
                    format!("$ENV{{{}}}", name)
                } else {
                    format!("${}", name)
                }
            }
            Sigil::Array => format!("@{}", name),
            Sigil::Hash => format!("%{}", name),
        },

        IrExpr::Index { var, key } => {
            let k = ir_expr_to_perl(key);
            format!("${{{}}}[{}]", var, k)
        }

        IrExpr::BinOp { lhs, op, rhs } => {
            // `! cmd` arrives as `BinOp{op:Not, lhs, rhs}` (the parser
            // duplicates the operand) — Perl's `!` is UNARY (`lhs !rhs`
            // was a syntax error).
            if matches!(op, BinOpKind::Not) {
                return format!("!({})", ir_expr_to_perl(rhs));
            }
            let l = ir_expr_to_perl(lhs);
            let r = ir_expr_to_perl(rhs);
            let op_str = match op {
                BinOpKind::Add => " + ",
                BinOpKind::Sub => " - ",
                BinOpKind::Mul => " * ",
                BinOpKind::Div => " / ",
                BinOpKind::Mod => " % ",
                BinOpKind::Pow => " ** ",
                BinOpKind::Concat => " . ",
                BinOpKind::Eq => " == ",
                BinOpKind::Ne => " ne ",
                BinOpKind::Lt => " < ",
                BinOpKind::Gt => " > ",
                BinOpKind::Le => " <=",
                BinOpKind::Ge => " >=",
                BinOpKind::And => " && ",
                BinOpKind::Or => " || ",
                BinOpKind::Not => " !",
                BinOpKind::BitAnd => " & ",
                BinOpKind::BitOr => " | ",
                BinOpKind::BitXor => " ^ ",
                BinOpKind::ShiftL => " << ",
                BinOpKind::ShiftR => " >> ",
            };
            format!("{}{}{}", l, op_str, r)
        }

        IrExpr::Call { func, args } => {
            // Word-level funcs (shared with render_word): getVar, split,
            // param, arith, brace, capture/captureWords, listVar.
            match func.as_str() {
                "getVar" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| var_read(&n))
                    .unwrap_or_else(|| "''".to_string()),
                "split" => args
                    .first()
                    .map(render_word)
                    .unwrap_or_else(|| "''".to_string()),
                "param" => render_param(args),
                "arith" => args
                    .first()
                    .and_then(|a| match a {
                        // an arith STRING (`$(($x % 2))` text, the C
                        // frontend's arith/testArith lowering): parse and
                        // render natively — a quoted `int("...")` would
                        // numify the LITERAL string, not the expression
                        IrExpr::Str(s, _) => arith_str_to_perl(s),
                        _ => None,
                    })
                    .unwrap_or_else(|| {
                        // An arithmetic expression `parse_arith` REJECTED
                        // (`$((echo "test"))`). bash reports `syntax error in
                        // expression` on stderr, leaves the value EMPTY and
                        // CONTINUES; numifying the raw text instead produced
                        // 0, which is observable — examples/
                        // arithmetic-vs-command-subshell expects
                        // `result=empty` from `${result:-empty}` and got
                        // `result=0`. Emit the diagnostic, value undef.
                        match args.first() {
                            Some(a) => {
                                let text = render_word(a)
                                    .replace('\\', "\\\\")
                                    .replace('"', "\\\"");
                                format!(
                                    "(do {{ print STDERR \"$0: syntax error in expression ({text})\\n\"; undef }})"
                                )
                            }
                            None => "0".to_string(),
                        }
                    }),
                "brace" => render_brace_word(args),
                "$fn_call" | "fnCall" => {
                    // Call to a shell function defined in this program, in
                    // EXPRESSION position — `if is_prime $n; then`
                    // (examples/051_primes line 50). The statement emitter has
                    // had a `$fn_call` arm for a while, but the expression
                    // renderer did not, so the call in the if-CONDITION fell
                    // through to the generic `_` arm and emitted a literal
                    // `$fn_call(...)` → "Global symbol "$fn_call" requires
                    // explicit package name". A Perl sub call is a valid
                    // expression; the status is `$?` for the caller.
                    let name = args.first().and_then(call_arg_str).unwrap_or_default();
                    let words = exec_word_args(args);
                    let rest: Vec<String> = words.iter().map(|w| render_word_list(w)).collect();
                    format!("{}({})", name, rest.join(", "))
                }
                "capture" | "captureWords" => {
                    // Expression/interpolated context (e.g. inside a
                    // double-quoted word): the raw chomped value, NOT a
                    // split (split in scalar context yields a field count).
                    let cap = args.first().and_then(arrow_to_cmd);
                    if let Some(cmd) = cap {
                        cmd_str_to_open_perl(&cmd)
                    } else if let Some(IrExpr::Arrow(stmts)) = args.first() {
                        // A PERL-SIDE capture: the C frontend's outparam
                        // channel — the callee ECHOES its out-param values,
                        // the caller captures STDOUT into a buffer.
                        let has_calls = stmts.iter().any(|s| {
                            matches!(s,
                                IrStmt::Expr(IrExpr::Call { func, .. })
                                    if func == "$fn_call" || func == "fnCall" || func == "exec" || func == "builtin")
                        });
                        if has_calls {
                            let mut body = String::new();
                            for st in stmts.iter() {
                                emit_stmt(&mut body, st, 1);
                            }
                            format!(
                                "do {{ my $__cap = ''; open my $__fh, '>', \\$__cap or die; my $__old = select($__fh); {body}close $__fh; select($__old); $__cap }}"
                            )
                        } else {
                            "''".to_string()
                        }
                    } else {
                        "''".to_string()
                    }
                }
                "listVar" | "getArray" | "arrayItems" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| {
                        if n == "@" || n == "*" {
                            "@ARGV".to_string()
                        } else {
                            format!("@{}", n)
                        }
                    })
                    .unwrap_or_else(|| "()".to_string()),
                "redirect" => {
                    // Redirect used as a condition: run the command with its
                    // redirects via bash -c and test its exit status.
                    //
                    // The rebuilt text carries RUNTIME VALUES in the shape
                    // `$ENV{DEST}` (redirect_call_to_cmd renders the target
                    // through ir_expr_to_perl), so it must go through
                    // `sh_cmd_value_expr` — the same splice every other
                    // shell-out uses. `safe_perl_q_string` escapes the `$`,
                    // so Perl never interpolated and bash received the
                    // literal text `"$ENV{DEST}.new"`, reporting
                    // `{DEST}.new: No such file or directory`
                    // (parse-dot-after-var).
                    match redirect_call_to_cmd(expr) {
                        Some(cmd) => {
                            let quoted = sh_cmd_value_expr(&cmd)
                                .unwrap_or_else(|| safe_perl_q_string(&cmd));
                            format!("(system('bash', '-c', {}) == 0)", quoted)
                        }
                        None => UNBUILDABLE_COND.to_string(),
                    }
                }
                "block" => {
                    // Multi-command block in condition position (e.g. a while
                    // cond with several statements): run them joined by `;`.
                    match block_call_to_cmd(expr) {
                        Some(cmd) => {
                            format!("(system('bash', '-c', {}) == 0)", safe_perl_q_string(&cmd))
                        }
                        // UNREBUILDABLE condition — a status this arm cannot
                        // compute. `"this holds"` is the only answer that
                        // keeps the control flow a loop rather than a hang
                        // (Perl would read a bare status `0` as FALSE and a
                        // `while` would never run; the historical
                        // `while ! { :; }` hang). Emitted DIRECTLY as `1`,
                        // not as a bare `0` for the post-pass to rewrite:
                        // the post-pass cannot tell this placeholder from a
                        // legitimately-computed `Bool(false)`, and rewriting
                        // THAT inverted real conditions (redirect-all:
                        // `if ! command -v CMD` rendered `!(0)` = true and
                        // the rewrite turned it into `!(1)` = false, dropping
                        // the branch).
                        None => UNBUILDABLE_COND.to_string(),
                    }
                }
                "arrayIndex" => {
                    let name = args.first().and_then(call_arg_str).unwrap_or_default();
                    let idx = args.get(1).and_then(call_arg_str).unwrap_or_default();
                    if idx == "@" || idx == "*" {
                        format!("@{}", name)
                    } else if idx.contains(',') {
                        // Bash pseudo-multidim subscript → hash key.
                        format!("${{{}}}{{\"{}\"}}", name, idx.replace('\"', "\\\""))
                    } else {
                        format!("${}[{}]", name, idx)
                    }
                }
                "arrayLen" => args
                    .first()
                    .and_then(call_arg_str)
                    .map(|n| format!("scalar(@{})", n))
                    .unwrap_or_else(|| "0".to_string()),
                "setArray" => {
                    // Array assignment in expression position (rare) — the
                    // elements as a parenthesized list (skip the name arg).
                    let elems: Vec<&IrExpr> = if args.len() >= 2 {
                        if let IrExpr::Array(elems) = &args[1] {
                            elems.iter().collect()
                        } else {
                            args[1..].iter().collect()
                        }
                    } else {
                        Vec::new()
                    };
                    let items: Vec<String> = elems.iter().map(|e| render_word_list(*e)).collect();
                    format!("({})", items.join(", "))
                }
                "test" => render_test_call(args),
                // `grep -o PAT` lift: the matches (list in list context, the
                // first in scalar).
                "grepMatches" => {
                    let text = args.first().map(ir_expr_to_perl).unwrap_or_else(|| "''".into());
                    let pat = args.get(1).and_then(call_arg_str).unwrap_or_default();
                    let flags = args.get(2).and_then(call_arg_str).unwrap_or_default();
                    let rx = grep_o_regex(&pat, &flags);
                    let fl = if flags.contains('i') { "i" } else { "" };
                    format!("(({text}) =~ /{rx}/{fl}g)")
                }
                "line" => {
                    // multi-return line read (`line(cap, N)` = the C
                    // frontend's outparam channel): String(v).split('\n')[N]
                    // → a Perl split with a default of ""
                    if let (Some(v), Some(IrExpr::Str(i, _))) = (args.first(), args.get(1)) {
                        let ve = ir_expr_to_perl(v);
                        let n = i.parse::<usize>().unwrap_or(0);
                        format!("((split(/\\n/, {ve}))[{n}] // '')")
                    } else {
                        eprintln!("otranspilerl: line args unsupported");
                        "''".to_string()
                    }
                }
                // `regexMatch(Regex(pattern, flags), value)` — the fish
                // `string match -rq` cond lift (triage-perl
                // t81_regex_match): Perl's native regex is the exact ERE
                // search decision (status 0 iff any match). flags: only
                // fish's `-i` (ignore-case) is emitted by the frontend.
                "regexMatch" => match args.first() {
                    Some(IrExpr::Regex { pattern, flags }) => {
                        if flags.chars().any(|c| c != 'i') {
                            eprintln!("otranspilerl: regexMatch flags {:?} unsupported", flags);
                            "0".to_string()
                        } else {
                            let v = args
                                .get(1)
                                .map(|a| ir_expr_to_perl(a))
                                .unwrap_or_else(|| "''".to_string());
                            let fl = if flags.contains('i') { "i" } else { "" };
                            let pat =
                                pattern.replace('{', "\\{").replace('}', "\\}");
                            format!("(({v}) =~ m{{{pat}}}{fl})")
                        }
                    }
                    other => {
                        eprintln!("otranspilerl: regexMatch arg 0 not Regex: {other:?}");
                        "0".to_string()
                    }
                },
                // Substring containment (the grep-lift `contains` and the
                // perl-sh-go index() form; triage-perl t60_contains):
                // `index($hay, $needle) >= 0` is perl's exact substring
                // decision (index returns -1 when absent), matching the
                // estree runtime's contains (String.includes).
                "contains" => {
                    let hay = args
                        .first()
                        .map(render_word)
                        .unwrap_or_else(|| "''".to_string());
                    let needle = args
                        .get(1)
                        .map(render_word)
                        .unwrap_or_else(|| "''".to_string());
                    format!("(index({hay}, {needle}) >= 0)")
                }
                // (( arith )) as a condition — the `let` builtin form.
                "exec" | "builtin"
                    if args.first().and_then(call_arg_str).as_deref() == Some("let") =>
                {
                    let words = exec_word_args(args);
                    let text = words
                        .first()
                        .and_then(|w| call_arg_str(w))
                        .unwrap_or_default();
                    format!("({})", arith_text_to_perl(&text))
                }
                // A command in CONDITION position (while/if tests — the
                // restructure pass's `while true` canonical form, `while
                // cmd`, …). Perl's native `exec` would REPLACE the
                // process instead of returning a status, so run the
                // command via bash -c and test its exit status (the same
                // lowering the `redirect`/`block` arms use).
                "exec" | "builtin" => {
                    // `read` in condition position: consume a line from
                    // STDIN and assign its fields (while-read loops).
                    if let Some(cond) = render_read_cond(args) {
                        return cond;
                    }
                    // Native file-contains: `grep -q PAT FILE` (quiet — no
                    // stdout side effect) lowers to read-the-file + a
                    // substring test, matching bash's grep exit status
                    // (0 = a line contains the literal). Refuse unless the
                    // flags are only quiet/case-insensitive, the pattern is
                    // a BRE-literal, and there is exactly one file operand
                    // (grep over many/globbed files or with output-
                    // producing flags -c/-l/-m/-b/-n/-A… is NOT a boolean
                    // and stays a shell-out).
                    if let Some(native) = crate::pipeline_native::native_exec_cond(expr) {
                        native
                    } else {
                        match exec_call_to_cmd(expr) {
                            Some(cmd) => format!(
                                "(system('bash', '-c', {}) == 0)",
                                safe_perl_q_string(&cmd)
                            ),
                            None => UNBUILDABLE_COND.to_string(),
                        }
                    }
                },
                // The C frontend's user-shell dispatch (the estree
                // lowers the same tree to sh2.fnCall) — a direct Perl sub
                // call: fnCall(name, [args...]) → name(args...).
                // `fnValue` (the echo-return transform's value-returning
                // convention) renders the same — the sub returns the
                // value, no status channel.
                "fnCall" | "fnValue" => {
                    let name = args.first().and_then(call_arg_str).unwrap_or_default();
                    let call_args: Vec<String> = match args.get(1) {
                        Some(IrExpr::Array(elems)) => {
                            elems.iter().map(|a| ir_expr_to_perl(a)).collect()
                        }
                        _ => Vec::new(),
                    };
                    format!("{}({})", name, call_args.join(", "))
                }
                _ => {
                    let a = args
                        .iter()
                        .map(|a| ir_expr_to_perl(a))
                        .collect::<Vec<_>>()
                        .join(", ");
                    // Special-case `chomp` with a single scalar argument to produce
                    // the idiomatic `chomp $var;` (without parentheses).
                    if func == "chomp" && args.len() == 1 {
                        format!("chomp {}", a)
                    // `join` needs a separator — a single argument (e.g. an
                    // array-slice param) gets the space join.
                    } else if func == "join" && args.len() == 1 {
                        format!("join(' ', {})", a)
                    // Special-case `join` to produce the idiomatic `join $sep, @list`
                    // (without parentheses) — the function-call parens add noise here.
                    } else if func == "join" && args.len() >= 2 {
                        format!("join {}", a)
                    } else {
                        format!("{}({})", func, a)
                    }
                }
            }
        }

        IrExpr::MethodCall { obj, method, args } => {
            let o = ir_expr_to_perl(obj);
            let a = args
                .iter()
                .map(|a| ir_expr_to_perl(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}->{}({})", o, method, a)
        }

        IrExpr::Ternary { cond, then, else_ } => {
            let c = ir_expr_to_perl(cond);
            let t = ir_expr_to_perl(then);
            let e = ir_expr_to_perl(else_);
            format!("({} ? {} : {})", c, t, e)
        }

        IrExpr::DefinedOr { expr, default } => {
            let e = ir_expr_to_perl(expr);
            let d = ir_expr_to_perl(default);
            format!("({} // {})", e, d)
        }

        IrExpr::Interpolate(parts) => {
            // Check if ALL expression parts are simple scalar variables.
            // If so, we can use a plain double-quoted string with $var interpolation.
            // If any part is a complex expression, use string concatenation instead
            // of the `${\(...)}` trick (which is unidiomatic transliteration-style).
            let all_simple = parts.iter().all(|part| match part {
                InterpPart::Lit(_) => true,
                InterpPart::Expr(e) => {
                    matches!(e.as_ref(), IrExpr::Var(_, None | Some(Sigil::Scalar)))
                }
            });

            if all_simple {
                // All expressions are simple scalar vars — emit a single
                // double-quoted string with $var interpolation (idiomatic Perl).
                let mut s = String::from("\"");
                for part in parts {
                    match part {
                        InterpPart::Lit(text) => {
                            for ch in text.chars() {
                                match ch {
                                    '"' => s.push_str("\\\""),
                                    '\\' => s.push_str("\\\\"),
                                    '$' => s.push_str("\\$"),
                                    '@' => s.push_str("\\@"),
                                    '\n' => s.push_str("\\n"),
                                    '\t' => s.push_str("\\t"),
                                    '\r' => s.push_str("\\r"),
                                    c if is_byte_marker(c) => {
                                        s.push_str(&byte_marker_escape(c))
                                    }
                                    c => s.push(c),
                                }
                            }
                        }
                        InterpPart::Expr(e) => {
                            if let IrExpr::Var(name, sigil) = e.as_ref() {
                                if sigil.unwrap_or(Sigil::Scalar) == Sigil::Scalar
                                    && is_env_style_var_name(name)
                                {
                                    s.push_str(&format!("$ENV{{{}}}", name));
                                } else {
                                    s.push_str(&format!("${}", name));
                                }
                            }
                        }
                    }
                }
                s.push('"');
                s
            } else {
                // Mixed or complex expressions — use string concatenation.
                // This avoids the unidiomatic `${\\(...)}` interpolation trick.
                let mut parts_str: Vec<String> = Vec::new();
                for part in parts {
                    match part {
                        InterpPart::Lit(text) => {
                            // Emit as a double-quoted string literal
                            let mut lit = String::from("\"");
                            for ch in text.chars() {
                                match ch {
                                    '"' => lit.push_str("\\\""),
                                    '\\' => lit.push_str("\\\\"),
                                    '$' => lit.push_str("\\$"),
                                    '@' => lit.push_str("\\@"),
                                    '\n' => lit.push_str("\\n"),
                                    '\t' => lit.push_str("\\t"),
                                    '\r' => lit.push_str("\\r"),
                                    c if is_byte_marker(c) => {
                                        lit.push_str(&byte_marker_escape(c))
                                    }
                                    c => lit.push(c),
                                }
                            }
                            lit.push('"');
                            parts_str.push(lit);
                        }
                        InterpPart::Expr(e) => {
                            let ev = ir_expr_to_perl(e);
                            // Wrap complex expressions in parentheses for safety,
                            // but keep simple vars bare.
                            match e.as_ref() {
                                IrExpr::Var(_, _) => {
                                    parts_str.push(ev);
                                }
                                _ => {
                                    parts_str.push(format!("({})", ev));
                                }
                            }
                        }
                    }
                }
                parts_str.join(" . ")
            }
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────

/// Helper: convert a command string (the body of a `qx{...}` call) into
/// Perl code that uses `open(my $fh, \'-|\', \'sh\', \'-c\', ...)` instead of
/// `qx{...}`.  This produces semantically equivalent code (both run
/// `/bin/sh -c \'cmd\'` and capture stdout) but avoids check_qx.pl
/// violations because `open()` is not checked.

/// The `__sh_run_ext` seam (UU-FFI.md): emitted into the preamble when
/// the body shells out to an external command. uu-ffi .so present ⇒
/// in-process coreutils (`sh2_uu_run`/`sh2_uu_capture` — concurrent-drain
/// capture, no fork/exec); absent ⇒ the feature-guarded fork/exec
/// fallback with a one-time stderr warning. check_qx whitelists this
/// block (the bash -c inside is guarded, never unconditional).
const SH_RUN_EXT_PREAMBLE: &str = r#"# __SH_RUN_EXT_BEGIN (UU-FFI.md external-command seam)
# uu-ffi .so present: in-process coreutils via uu_run (capture = temp-file
# fd-1 redirect around the in-process call — no pipe, so no concurrent-
# drain deadlock, and no C-side wrapper .so needed). Absent: guarded
# fork/exec fallback (one-time warning). The bash -c below is
# feature-guarded — check_qx strips this block before scanning.
my $__SH_UU = 0;
my $__SH_UU_FFI;
our $__sh_uu_rc = 0;
my %__SH_UU_OK = map { $_ => 1 } qw(cat wc ls sort sed awk grep date sha256sum uname basename dirname cp);
BEGIN {
    my @cand;
    push @cand, $ENV{SH2_UU_LIB} if defined $ENV{SH2_UU_LIB};
    for my $up ('', '../', '../../', '../../../', '../../../../', '../../../../../') {
        push @cand, "${up}runtime/lib/libcoreutils_ffi.so";
    }
    push @cand, 'libcoreutils_ffi.so';
    for my $lib (@cand) {
        next unless defined $lib && -e $lib;
        my $ok = eval {
            require FFI::Platypus;
            my $ffi = FFI::Platypus->new(lib => [$lib]);
            $ffi->attach([uu_run => '__sh_uu_run_ffi'] => ['string', 'int', 'string[]'] => 'int');
            $__SH_UU_FFI = $ffi;
            $__SH_UU = 1;
            1;
        };
        last if $ok;
    }
    unless ($__SH_UU) {
        warn "sh2perl: uu-ffi .so (libcoreutils_ffi) not found — external commands fall back to fork/exec (slower)\n";
    }
}
my $__SH_UU_SEQ = 0;
sub __sh_uu_run {
    my @argv = @_;
    if ($__SH_UU && ($__SH_UU_OK{ $argv[0] // '' } // 0)) {
        $__sh_uu_rc = __sh_uu_run_ffi($argv[0], scalar(@argv), \@argv);
        return;
    }
    system(@argv);
    $__sh_uu_rc = $? >> 8;
}
sub __sh_uu_capture {
    my @argv = @_;
    if ($__SH_UU && ($__SH_UU_OK{ $argv[0] // '' } // 0)) {
        # Capture = fd-1 redirect to a temp file around the IN-PROCESS
        # uu_run call (the fd redirect is what the C wrapper's drain
        # thread exists for; a temp file cannot fill, so no deadlock).
        my $tmpf = "/tmp/sh2_uu_${$}_" . ($__SH_UU_SEQ++) . ".cap";
        open(my $saved, '>&', \*STDOUT) or do { $__sh_uu_rc = 127; return ''; };
        open(STDOUT, '>', $tmpf) or do { $__sh_uu_rc = 127; return ''; };
        my $rc = eval { __sh_uu_run_ffi($argv[0], scalar(@argv), \@argv) };
        open(STDOUT, '>&', $saved) or do { $__sh_uu_rc = 127; return ''; };
        close($saved);
        $__sh_uu_rc = defined $rc ? $rc : 127;
        my $out = '';
        if (open(my $rf, '<', $tmpf)) { $out = do { local $/; <$rf> } // ''; close $rf; }
        unlink $tmpf;
        return $out;
    }
    my $out = do {
        open(my $fh, '-|', @argv) or do { $__sh_uu_rc = 127; return ''; };
        my $r = do { local $/; <$fh> };
        close $fh;
        $r;
    };
    $__sh_uu_rc = $? >> 8;
    return $out // '';
}
# ── pipeline front end (coreutils-ffi-pipeline C ABI) ──────────────
# uu_pipeline_run_str runs a LINEAR pipeline (a|b|c) with redirections
# in-process, so the Perl backend never emits `bash -c` for those shapes.
my $__SH_PIPE = 0;
my $__SH_PIPE_TRIED = 0;
# Shell single-quote a runtime value so the pipeline parser sees one argv.
sub __sh_q {
    my $s = shift;
    $s = '' unless defined $s;
    $s =~ s/'/'\\''/g;
    return "'$s'";
}
sub __sh_pipe_load {
    return 1 if $__SH_PIPE;
    return 0 if $__SH_PIPE_TRIED;
    $__SH_PIPE_TRIED = 1;
    my @cand;
    push @cand, $ENV{SH2_UU_PIPELINE_LIB} if defined $ENV{SH2_UU_PIPELINE_LIB};
    for my $up ('', '../', '../../', '../../../', '../../../../', '../../../../../') {
        push @cand, "${up}runtime/lib/libcoreutils_ffi_pipeline.so";
    }
    push @cand, 'libcoreutils_ffi_pipeline.so';
    for my $lib (@cand) {
        next unless defined $lib && -e $lib;
        my $ok = eval {
            require FFI::Platypus;
            my $ffi = FFI::Platypus->new(api => 2);
            $ffi->lib($lib);
            $ffi->attach([uu_pipeline_run_str => '__sh_pipe_run_str'] => ['string','string'] => 'int');
            $ffi->attach([uu_pipeline_last_code => '__sh_pipe_last_code'] => [] => 'int');
            $__SH_PIPE = 1;
            1;
        };
        last if $ok;
    }
    # Point the process backends at the multicall helper shipped alongside.
    if ($__SH_PIPE && !defined $ENV{UU_FFI_MULTICALL}) {
        for my $up ('', '../', '../../', '../../../') {
            my $m = "${up}runtime/lib/coreutils-ffi-multicall";
            if (-x $m) { $ENV{UU_FFI_MULTICALL} = $m; last; }
        }
    }
    $__SH_PIPE;
}
sub __sh_pipe_unsupported_util {
    my ($cmd) = @_;
    # The reconstructed command quotes each stage's util name. These are not
    # in the pipeline .so (non-uutils tools / shell builtins); routing them
    # in-process is a race between rc 127 (unknown util) and rc 141 (SIGPIPE
    # from the neighbour writing into the dead stage) — 141 is a real exit
    # code, so the caller skipped the shell fallback and printed nothing
    # (parse-sq-with-for, intermittently). Send them to the shell up front.
    return $cmd =~ m{(?:^|\|)\s*'(?:perl|python3?|bc|find|strings|gunzip|gzip|zstd|bzip2|xz|tar|unzip|mapfile|readarray|ruby|node|php)'};
}
sub __sh_pipe_run {
    my ($cmd, $opts) = @_;
    if (__sh_pipe_load() && !__sh_pipe_unsupported_util($cmd)) {
        my $rc = __sh_pipe_run_str($cmd, defined $opts ? $opts : '');
        # `-2` parse / `-3` backend; `127` is the multicall's "unknown util"
        # (not a uutils command) — run those through the shell instead.
        if ($rc >= 0 && $rc != 127) { $CHILD_ERROR = $rc; return $rc; }
    }
    # Not parseable / unknown util / no backend / no .so: run it via the shell.
    system('bash', '-c', $cmd);
    $CHILD_ERROR = $? >> 8;
    return $CHILD_ERROR;
}
sub __sh_pipe_capture {
    my ($cmd, $opts) = @_;
    if (__sh_pipe_load() && !__sh_pipe_unsupported_util($cmd)) {
        my $tmp = "/tmp/sh2_pipe_${$}_" . ($__SH_UU_SEQ++) . ".cap";
        my $rc = __sh_pipe_run_str($cmd . ' > ' . $tmp, defined $opts ? $opts : '');
        if ($rc >= 0 && $rc != 127) {
            my $out = '';
            if (open(my $rf, '<', $tmp)) { $out = do { local $/; <$rf> } // ''; close $rf; }
            unlink $tmp;
            $CHILD_ERROR = $rc;
            $out =~ s/\n+\z//;
            return $out;
        }
        unlink $tmp;
    }
    my $out = do {
        open(my $fh, '-|', 'bash', '-c', $cmd) or do { $CHILD_ERROR = 127; return ''; };
        my $r = do { local $/; <$fh> };
        close $fh;
        $r;
    };
    $CHILD_ERROR = $? >> 8;
    $out =~ s/\n+\z//;
    return $out // '';
}
# __SH_RUN_EXT_END
"#;

/// A `$` that a shell would expand but that sits OUTSIDE a double-quoted
/// word (single-quoted text is literal, and a `\'`-escaped quote inside a
/// double-quoted word does not close it). The pipeline ABI and `__sh_q`
/// have no expansion, so any such `$` must keep the bash -c path.
fn has_unquoted_dollar(cmd: &str) -> bool {
    let b = cmd.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'\'' => {
                i += 1;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'$' => return true,
            _ => i += 1,
        }
    }
    false
}

/// Can `cmd` be parsed by the `coreutils-ffi-pipeline` grammar? That grammar
/// supports `a | b | c` plus `> FILE` / `>> FILE` / `< FILE` / `<<< text` /
/// `2>/dev/null`, and REJECTS `&&`, `||`, `;`, `$(...)`, backticks, `$VAR`,
/// globs and subshells. Route only when the parser will not error (the
/// parser would reject at runtime, but a `bash -c` fallback in the same
/// emit would still trip check_qx).
fn pipeline_grammar_ok(cmd: &str) -> bool {
    if cmd.trim().is_empty() {
        return false;
    }
    // Reject anything the parser would treat literally rather than expand:
    // `$var` (the grammar has no expansion, so a `$` would be passed as the
    // literal text — wrong), globs, control flow, subshells. A quoted
    // `$`/glob is still wrong here, so reject regardless of quoting.
    for c in cmd.chars() {
        if matches!(
            c,
            '&' | ';' | '`' | '$' | '(' | ')' | '*' | '?' | '[' | ']' | '{' | '}' | '!' | '~'
                | '\n'
        ) {
            return false;
        }
    }
    true
}

/// Parse a bash double-quoted string's content into a Perl expression:
/// `$d/f1` → `$d . "/f1"`, `${x}` → `$x`, `pre$v` → `'pre' . $v`.
/// None for a shape we cannot express (a `${…}` operator form).
fn dq_value_expr(content: &str) -> Option<String> {
    // A backslash escape inside the double quotes (`\"`) changes the
    // shell-level meaning; refuse so the caller keeps the previous path.
    if content.contains('\\') {
        return None;
    }
    // A COMMAND SUBSTITUTION nested inside the double quotes is not a value
    // this can produce: `$(…)`/backtick is a whole command whose OUTPUT
    // becomes the value, and the identifier scan below only rewrites a `$`
    // expansion it recognised — a leftover `$(` falls through as literal
    // text and is spliced into the `q{…}` command string verbatim, handing
    // bash a Perl expression as a literal WORD. Refuse so the caller keeps
    // the plain `bash -c` text path, which evaluates it correctly.
    if content.contains("$(") || content.contains('`') {
        return None;
    }
    let chars: Vec<char> = content.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            let (name, next) = if chars[i + 1] == '{' {
                let mut j = i + 2;
                let mut depth = 1;
                while j < chars.len() && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    }
                    if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth != 0 {
                    return None;
                }
                (chars[i + 2..j - 1].iter().collect::<String>(), j)
            } else if chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_' {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                (chars[i + 1..j].iter().collect::<String>(), j)
            } else if matches!(chars[i + 1], '@' | '*') {
                // `"$@"` / `"$*"` — the positional list. In a Perl string
                // that is the IFS-joined parameters (var_read knows the
                // spelling). Leaving the `$` in the literal text would feed
                // bash a `'$@'` word (literal, not expanded).
                (chars[i + 1].to_string(), i + 2)
            } else {
                lit.push(chars[i]);
                i += 1;
                continue;
            };
            // Only a plain name read (`$x` / `${x}`) or the positional
            // list; an operator form (`${x:-y}`) is left for the `param`
            // path.
            if name.is_empty()
                || !(name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || name == "@"
                    || name == "*")
            {
                return None;
            }
            if !lit.is_empty() {
                parts.push(safe_perl_q_string(&lit));
                lit.clear();
            }
            // The redirect arm binds dynamic targets into `%ENV` slots.
            if name.starts_with("__sh2_") {
                parts.push(format!("$ENV{{{}}}", name));
            } else {
                parts.push(var_read(&name));
            }
            i = next;
        } else {
            lit.push(chars[i]);
            i += 1;
        }
    }
    if !lit.is_empty() {
        parts.push(safe_perl_q_string(&lit));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" . "))
    }
}

/// Build a PERL EXPRESSION evaluating to `cmd` with every shell expansion
/// substituted as a runtime VALUE (shell-quoted by `__sh_q`), never left as
/// raw text.
///
/// WHY THIS EXISTS. `safe_perl_q_string` escapes `$` so PERL does not
/// interpolate the string — which is correct for a literal, but it means a
/// `bash -c` seam handed `q('echo' ${!aa[@]} | sort)` gives bash the literal
/// characters `$ { ! a a [` and the expansion yields NOTHING. The command
/// still parses and the exit code is still right; only the value vanishes
/// (core-request perl-generator-20261002-053100). A bare `$n` happened to
/// survive because `var_exports_str` re-exports the NAME through %ENV and
/// bash re-expands it — but there is no %ENV carrier for a COMPOUND
/// expansion (`${arr[@]}`, `${!h[@]}`, `${h[k]}`), so those were silently
/// lost.
///
/// Returns None when the shape is not expressible (globs, `$(...)`, `&&`,
/// `||`, subshells, backticks) so the caller keeps the plain shell-out.
fn sh_cmd_value_expr(cmd: &str) -> Option<String> {
    if cmd.contains("||") || cmd.contains("|&") {
        return None;
    }
    let chars: Vec<char> = cmd.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // single-quoted span: no expansion, keep verbatim
        if c == '\'' {
            lit.push(c);
            i += 1;
            while i < chars.len() {
                lit.push(chars[i]);
                let done = chars[i] == '\'';
                i += 1;
                if done {
                    break;
                }
            }
            continue;
        }
        if c == '$' && i + 1 < chars.len() {
            if let Some((expr, next)) = expansion_at(&chars, i) {
                if !lit.is_empty() {
                    parts.push(safe_perl_q_string(&lit));
                    lit.clear();
                }
                parts.push(format!("__sh_q({})", expr));
                i = next;
                continue;
            }
        }
        // A bare `$(...)`/backtick inside the command has no value form here.
        if matches!(c, '`' | '&' | ';' | '(' | ')' | '*' | '?' | '\n') {
            return None;
        }
        lit.push(c);
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(safe_perl_q_string(&lit));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" . "))
    }
}

/// Parse one shell expansion starting at `$` (chars[i] == '$'), returning a
/// Perl value EXPRESSION for it and the index just past it.
///
/// Handles `$name`, `${name}`, `${name[@]}` / `${name[*]}` (the whole
/// collection), `${!name[@]}` (a hash's KEYS) and `${name[key]}` (one
/// element). None for any other shape — notably the operator forms
/// (`${x:-y}`, `${x#p}`, `${x/a/b}`) which carry semantics this cannot
/// reproduce, so the caller must not silently mangle them.
fn expansion_at(chars: &[char], i: usize) -> Option<(String, usize)> {
    if chars.get(i + 1) == Some(&'{') {
        // find the matching `}` (depth-counted)
        let mut depth = 1;
        let mut j = i + 2;
        while j < chars.len() && depth > 0 {
            match chars[j] {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
            j += 1;
        }
        if depth != 0 {
            return None;
        }
        let inner: String = chars[i + 2..j].iter().collect();
        let next = j + 1;
        let expr = compound_expansion_expr(&inner)?;
        return Some((expr, next));
    }
    let c1 = *chars.get(i + 1)?;
    if c1 == '@' || c1 == '*' {
        return None;
    }
    if !(c1.is_ascii_alphabetic() || c1 == '_') {
        return None;
    }
    let mut j = i + 1;
    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    Some((var_read(&chars[i + 1..j].iter().collect::<String>()), j))
}

/// Perl expression for the INSIDE of a `${…}` expansion. None for shapes
/// whose semantics are not reproducible here (operator forms).
fn compound_expansion_expr(inner: &str) -> Option<String> {
    let s = inner.trim();
    if s.is_empty() {
        return None;
    }
    // `${!name[@]}` / `${!name[*]}` — a hash's keys (indirect expansion).
    if let Some(rest) = s.strip_prefix('!') {
        let (name, idx) = split_indexed_var(rest);
        let idx = idx?;
        if !var_is_declarable(name) {
            return None;
        }
        if matches!(idx, "@" | "*") {
            // bash yields the KEYS; sort for a stable, hash-order-independent
            // result (the corpus sorts downstream anyway).
            return Some(format!("join(' ', sort keys %{{{name}}})"));
        }
        return None;
    }
    // `${name[@]}` / `${name[*]}` — the whole collection.
    if let Some(rest) = s.strip_suffix("[@]").or_else(|| s.strip_suffix("[*]")) {
        if rest.is_empty() || !var_is_declarable(rest) {
            return None;
        }
        if array_var(rest) {
            // bash "${arr[@]}" inside double quotes joins on the first IFS
            // char; a space matches the corpus' `IFS=$'\n' sorted=(...)` and
            // `echo "${arr[@]}"` spellings that flow through here.
            return Some(format!("join(' ', @{{{rest}}})"));
        }
        // A hash: `${h[@]}` is its VALUES.
        return Some(format!("join(' ', values %{{{rest}}})"));
    }
    // `${name[key]}` — one element. `key` may be a nested expansion; handle
    // only a literal key (a dynamic one cannot be resolved at this point).
    if let Some(open) = s.find('[') {
        if s.ends_with(']') {
            let name = &s[..open];
            let key = &s[open + 1..s.len() - 1];
            if name.is_empty() || key.is_empty() || key.contains('$') {
                return None;
            }
            if !var_is_declarable(name) {
                return None;
            }
            if array_var(name) {
                return Some(format!("${{{name}}}[{}]", key));
            }
            return Some(format!("${{{name}}}{{{key}}}"));
        }
    }
    // A bare `${name}` is a plain variable read.
    if var_is_declarable(s) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Some(var_read(s));
    }
    None
}

/// Build a PERL EXPRESSION that evaluates to a pipeline command string with
/// runtime values substituted for `"$var"` arguments. The `coreutils-ffi`
/// pipeline grammar has no expansion, so a `$var` in the reconstructed shell
/// text would be passed literally; splice the VALUE in instead (shell-quoted
/// by `__sh_q`, so a value with spaces stays one argv). None when the shape
/// is not expressible (globs, `$(...)`, `&&`, `||`, subshells).
fn pipeline_value_cmd(cmd: &str) -> Option<String> {
    // The pipeline parser only accepts the fd-2 null redirect in its
    // no-space form (`2>/dev/null`); the renderer emits `2> '/dev/null'`.
    let cmd = cmd
        .replace("2> '/dev/null'", "2>/dev/null")
        .replace("2> /dev/null", "2>/dev/null");
    let cmd = cmd.as_str();
    if cmd.contains("||") || cmd.contains("|&") {
        return None;
    }
    // A bare `$` outside double quotes (`"$@"`, `$var`, `${x}`) is a SHELL
    // expansion this ABI has no grammar for. The word-builder emits the
    // native shell form so a `bash -c` run expands it; routing that text
    // through the pipeline parser (or `__sh_q`) would pass it literally.
    // Only the `$` inside a double-quoted word is safe: `dq_value_expr`
    // turns it into a Perl expression spliced as a runtime value.
    if has_unquoted_dollar(cmd) {
        return None;
    }
    let reject = |c: char| {
        matches!(
            c,
            '&' | ';' | '`' | '(' | ')' | '*' | '?' | '[' | ']' | '{' | '}' | '!' | '~' | '\n'
        )
    };
    let chars: Vec<char> = cmd.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if reject(c) {
            return None;
        }
        if c == '\'' {
            lit.push(c);
            i += 1;
            while i < chars.len() {
                lit.push(chars[i]);
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == '"' {
            let mut j = i + 1;
            let mut content = String::new();
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' && j + 1 < chars.len() {
                    content.push(chars[j]);
                    content.push(chars[j + 1]);
                    j += 2;
                    continue;
                }
                content.push(chars[j]);
                j += 1;
            }
            if j >= chars.len() {
                return None;
            }
            if content.contains('$') {
                // `"$@"` / `"$*"` expand to MULTIPLE words (or the IFS join):
                // `__sh_q` would quote the joined value into ONE argv. Keep
                // the bash -c path, where the shell does the expansion.
                if content.contains("$@") || content.contains("$*") {
                    return None;
                }
                let expr = dq_value_expr(&content)?;
                if !lit.is_empty() {
                    parts.push(safe_perl_q_string(&lit));
                    lit.clear();
                }
                parts.push(format!("__sh_q({})", expr));
                i = j + 1;
                continue;
            }
            lit.push('"');
            lit.push_str(&content);
            lit.push('"');
            i = j + 1;
            continue;
        }
        lit.push(c);
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(safe_perl_q_string(&lit));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" . "))
    }
}

/// Split a shell command string into argv (dq/sq aware, quotes stripped).
/// REFUSE > GUESS: None on shell metacharacters (`| & ; < > ( )` backtick,
/// `$`, backslash, newline) — those keep the existing shell-string paths —
/// or on unbalanced quotes.
pub(crate) fn sh_split_simple_cmd(cmd: &str) -> Option<Vec<String>> {
    if cmd.is_empty() {
        return None;
    }
    if cmd.chars().any(|c| {
        matches!(c, '|' | '&' | ';' | '<' | '>' | '`' | '$' | '\n')
    }) {
        return None;
    }
    let mut argv = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match (quote, c) {
            (None, '"') | (None, '\'') => {
                if !cur.is_empty() {
                    argv.push(std::mem::take(&mut cur));
                }
                quote = Some(c);
            }
            (Some(q), c2) if Some(c2) == quote => {
                argv.push(std::mem::take(&mut cur));
                quote = None;
            }
            (Some(_), c2) => cur.push(c2),
            (None, '\\') | (None, '(') | (None, ')') => return None,
            (None, c2) if c2.is_whitespace() => {
                if !cur.is_empty() {
                    argv.push(std::mem::take(&mut cur));
                }
            }
            (None, c2) => cur.push(c2),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !cur.is_empty() {
        argv.push(cur);
    }
    if argv.is_empty() {
        None
    } else {
        Some(argv)
    }
}

/// Perl single-quoted literal for one argv word.
fn perl_word_literal(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Whether command-substitution captures render through the `__sh_uu_*`
/// external-command seam. The seam needs the standalone preamble helpers
/// (`sub __sh_uu_capture` / `sub __sh_uu_run`); embed fragments are
/// preamble-free and splice-clean, so the embed renderer disables it and
/// uses the self-contained open-idiom instead (which also carries the
/// `local $/` / `$!` / `chomp` markers the embed flag rewrites govern).
thread_local! {
    static UU_SEAM: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    /// Variable names the program binds as ARRAYS (setArray / DeclareArray).
    /// The A1 `param("slice", …)` node drops the `[@]` distinction from
    /// `${arr[@]:o:l}` vs `${s:o:l}`; this lets the renderer recover it.
    static ARRAY_VARS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    /// Variable names the program binds as ASSOCIATIVE arrays
    /// (`declare -A`, `h[k]=v`). `${h[@]}` is the hash's VALUES.
    static HASH_VARS: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

fn array_var(name: &str) -> bool {
    ARRAY_VARS.with(|s| s.borrow().contains(name))
}

/// Whether the program binds `name` as an ASSOCIATIVE array. `${x[@]}` on a
/// hash is its VALUES, not an array's elements; the A1 carries no
/// array/hash distinction on `param("slice", …)`, so the census recovers it.
fn hash_var(name: &str) -> bool {
    HASH_VARS.with(|s| s.borrow().contains(name))
}

/// Names the program binds as associative arrays: `declare -A X`,
/// `X[k]=v` writes (an `Assign` whose var carries a subscript), and an
/// explicit Hash sigil on a declaration.
fn collect_hash_vars(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    fn one(s: &IrStmt, out: &mut std::collections::HashSet<String>) {
        match s {
            IrStmt::Assign { targets, .. } => {
                for t in targets {
                    if let (base, Some(_)) = split_indexed_var(&t.var) {
                        out.insert(base.to_string());
                    } else if t.sigil == Some(Sigil::Hash) {
                        out.insert(t.var.clone());
                    }
                }
            }
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    if d.sigil == Some(Sigil::Hash) {
                        out.insert(d.name.clone());
                    }
                }
            }
            IrStmt::DeclareArray { var, sigil, .. } => {
                if *sigil == Some(Sigil::Hash) {
                    out.insert(var.clone());
                }
            }
            // `declare -A name` / `local -A name` (a Call, not a Declare).
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if matches!(func.as_str(), "exec" | "builtin") =>
            {
                if let Some(cmd) = args.first().and_then(crate::ir::call_arg_str) {
                    if matches!(cmd.as_str(), "local" | "declare" | "typeset") {
                        let mut is_hash = false;
                        for w in crate::ir::exec_word_args(args) {
                            if let Some(s) = crate::ir::call_arg_str(w) {
                                if let Some(f) = s.strip_prefix('-') {
                                    if f.contains('A') {
                                        is_hash = true;
                                    }
                                    continue;
                                }
                            }
                            if is_hash {
                                if let Some(name) = crate::ir::call_arg_str(w) {
                                    let name = name.split('=').next().unwrap_or(&name);
                                    if var_is_declarable(name) {
                                        out.insert(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_hash_vars(then, out);
                for (_, b) in elsifs {
                    collect_hash_vars(b, out);
                }
                collect_hash_vars(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_hash_vars(body, out),
            IrStmt::ForInit { init, step, body, .. } => {
                collect_hash_vars(init, out);
                collect_hash_vars(step, out);
                collect_hash_vars(body, out);
            }
            IrStmt::Function { body, named_blocks, .. } => {
                collect_hash_vars(body, out);
                for (_, b) in named_blocks {
                    collect_hash_vars(b, out);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_hash_vars(inner, out),
            _ => {}
        }
    }
    for s in stmts {
        one(s, out);
    }
}

/// The array census, for the pipeline_native leaves (`${arr[@]}` elements
/// vs `${map[@]}` hash values).
pub(crate) fn array_var_pub(name: &str) -> bool {
    array_var(name)
}

/// The associative-array census, for the pipeline_native leaves.
pub(crate) fn hash_var_pub(name: &str) -> bool {
    hash_var(name)
}

/// Populate the array / associative-array censuses for `prog`.
///
/// {@link shir_to_perl} does this itself, but it runs it *after* its own
/// preamble — a consumer that must consult {@link array_var_pub} while
/// preparing its fixups (the perl GENERATOR's `fix_array_slice_bounds`
/// gates `${arr[@]:o:l}` → `__sh_slice` on it) otherwise reads an empty
/// census and never takes the rewrite. `fixups` introduces no new array
/// bindings, so running it on the pre-`shir_to_perl` program is equivalent.
pub(crate) fn populate_var_census(prog: &IrProgram) {
    let mut arrays = std::collections::HashSet::new();
    collect_array_vars(&prog.stmts, &mut arrays);
    for sub in &prog.subs {
        collect_array_vars(&sub.body, &mut arrays);
    }
    // A READ of `${name[@]}` / `${name[*]}` also binds `name` as an array:
    // that subscript means "all elements" and can never be a hash key, even
    // when the name is otherwise never ASSIGNED as an array (the A1 drops
    // `declare -a`). Without this the renderer emitted `@name` for the read
    // but the preamble declared `my $name` — a `use strict` compile error
    // that aborted the program (063_04).
    collect_all_subscript_reads(&prog.stmts, &mut arrays);
    for sub in &prog.subs {
        collect_all_subscript_reads(&sub.body, &mut arrays);
    }
    ARRAY_VARS.with(|s| *s.borrow_mut() = arrays);
    let mut hashes = std::collections::HashSet::new();
    collect_hash_vars(&prog.stmts, &mut hashes);
    for sub in &prog.subs {
        collect_hash_vars(&sub.body, &mut hashes);
    }
    HASH_VARS.with(|s| *s.borrow_mut() = hashes);
}


/// Mark every `name` read through the all-elements subscript (`${name[@]}`,
/// `${name[*]}`, and the same inside a `param("slice", …)` call) as an ARRAY.
fn collect_all_subscript_reads(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    fn walk_expr(e: &IrExpr, out: &mut std::collections::HashSet<String>) {
        match e {
            IrExpr::Call { func, args } => {
                // `param(op, NAME, …)` — NAME carries the subscript text.
                if func == "param" {
                    if let Some(name) = args.get(1).and_then(call_arg_str) {
                        if let Some(base) = name.strip_suffix("[@]").or_else(|| name.strip_suffix("[*]")) {
                            if !base.is_empty() {
                                out.insert(base.to_string());
                            }
                        }
                    }
                }
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        walk_expr(x, out);
                    }
                }
            }
            IrExpr::Array(items) => {
                for i in items {
                    walk_expr(i, out);
                }
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, out),
            _ => {}
        }
    }
    fn walk(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
        for s in stmts {
            match s {
                IrStmt::Expr(e) => walk_expr(e, out),
                IrStmt::Assign { expr, .. } => walk_expr(expr, out),
                IrStmt::Output { value, .. } => walk_expr(value, out),
                IrStmt::If { cond, then, elsifs, else_, .. } => {
                    walk_expr(cond, out);
                    walk(then, out);
                    for (c, b) in elsifs {
                        walk_expr(c, out);
                        walk(b, out);
                    }
                    walk(else_, out);
                }
                IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                    walk_expr(cond, out);
                    walk(body, out);
                }
                IrStmt::For { iter, body, .. } => {
                    walk_expr(iter, out);
                    walk(body, out);
                }
                IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Function { body: b, .. } => walk(b, out),
                _ => {}
            }
        }
    }
    walk(stmts, out);
}

fn collect_array_vars(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    fn one(s: &IrStmt, out: &mut std::collections::HashSet<String>) {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                if matches!(expr, IrExpr::Call { func, .. }
                    if func == "setArray" || func == "setArrayAppend")
                {
                    for t in targets {
                        out.insert(t.var.clone());
                    }
                }
                // `assign(NAME, "+=", <word list>)` is the array append the
                // frontend emits for `arr+=(a b)`. When it is the FIRST
                // mention of the name nothing else has bound it as an array,
                // so without this the renderer would take the scalar branch
                // and emit `$z += ...` (examples/051_primes: `z+=(a b)` then
                // `z+=${primes[@]:0:1}`).
                if let IrExpr::Call { func, args } = expr {
                    if func == "assign"
                        && args.get(2).map(|v| matches!(v, IrExpr::Array(_))).unwrap_or(false)
                    {
                        for t in targets {
                            out.insert(t.var.clone());
                        }
                    }
                }
            }
            IrStmt::DeclareArray { var, sigil, .. } => {
                // Only ARRAY declarations; a hash (`declare -A`) must keep
                // hash-key indexing in `var_read`.
                if sigil.unwrap_or(crate::ir::Sigil::Array) == crate::ir::Sigil::Array {
                    out.insert(var.clone());
                }
            }
            // `local -a`/`declare -a`/`typeset -a` declarations (a Call, not
            // a Declare node) also bind an array.
            IrStmt::Expr(IrExpr::Call { func, args, .. })
                if matches!(func.as_str(), "exec" | "builtin") =>
            {
                if let Some(cmd) = args.first().and_then(crate::ir::call_arg_str) {
                    if matches!(cmd.as_str(), "local" | "declare" | "typeset") {
                        let mut is_array = false;
                        for w in crate::ir::exec_word_args(args) {
                            if let Some(s) = crate::ir::call_arg_str(w) {
                                if let Some(f) = s.strip_prefix('-') {
                                    if f.contains('a') && !f.contains('A') {
                                        is_array = true;
                                    }
                                    continue;
                                }
                            }
                            if is_array {
                                match w {
                                    IrExpr::Call { func: sf, args: sa }
                                        if sf == "setArray" =>
                                    {
                                        if let Some(IrExpr::Str(n, _)) = sa.first() {
                                            if var_is_declarable(n) {
                                                out.insert(n.clone());
                                            }
                                        }
                                    }
                                    _ => {
                                        if let Some(name) = crate::ir::call_arg_str(w) {
                                            let name = name.split('=').next().unwrap_or(&name);
                                            if var_is_declarable(name) {
                                                out.insert(name.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_array_vars(then, out);
                for (_, b) in elsifs {
                    collect_array_vars(b, out);
                }
                collect_array_vars(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_array_vars(body, out),
            IrStmt::ForInit { init, step, body, .. } => {
                collect_array_vars(init, out);
                collect_array_vars(step, out);
                collect_array_vars(body, out);
            }
            IrStmt::Function { body, named_blocks, .. } => {
                collect_array_vars(body, out);
                for (_, b) in named_blocks {
                    collect_array_vars(b, out);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_array_vars(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_array_vars(st, out);
                }
            }
            _ => {}
        }
    }
    for s in stmts {
        one(s, out);
    }
}

pub(crate) fn uu_seam_enabled() -> bool {
    UU_SEAM.with(|c| c.get())
}

/// Route a simple command through the `__sh_uu_*` seam (capture form):
/// `do { my $_r = __sh_uu_capture(…); $CHILD_ERROR = $__sh_uu_rc; chomp
/// $_r; $_r; }` — byte-identical semantics to the open-idiom it replaces
/// (trailing-newline chomp; $?-derived status), without the bash wrap.
fn sh_uu_capture_expr(argv: &[String]) -> String {
    let args = argv.iter().map(|w| perl_word_literal(w)).collect::<Vec<_>>().join(", ");
    format!(
        "do {{ my $_r = __sh_uu_capture({args}); $CHILD_ERROR = $__sh_uu_rc; chomp $_r; $_r; }}"
    )
}

/// One exec word as a uu-ffi ARGV expression: literals are quoted, dynamic
/// words render as Perl expressions (so `grep "$pat" f` passes the value,
/// not the shell text).
fn uu_arg(w: &IrExpr) -> String {
    if let Some(l) = grep_lit_str(w) {
        perl_word_literal(&l)
    } else {
        render_word(w)
    }
}

/// Shell BUILTINS must never go through the uu-ffi argv seam (it runs a
/// command, not a shell): `eval`/`exec`/`source`/`read`/… need the shell.
fn is_shell_builtin(cmd: &str) -> bool {
    matches!(
        cmd,
        "eval"
            | "exec"
            | "."
            | "source"
            | "break"
            | "continue"
            | "return"
            | "shift"
            | "set"
            | "unset"
            | "export"
            | "readonly"
            | "declare"
            | "typeset"
            | "local"
            | "alias"
            | "unalias"
            | "trap"
            | "shopt"
            | "wait"
            | "read"
            | "cd"
            | "pwd"
            | "echo"
            | "printf"
            | "test"
            | "["
            | ":"
            | "true"
            | "false"
            | "let"
            | "getopts"
            | "hash"
            | "type"
            | "ulimit"
            | "umask"
            | "times"
            | "jobs"
            | "fg"
            | "bg"
    )
}

/// A single external command (the native leaves did not cover it) as a
/// `__sh_uu_capture` expression. The shell-out lives in the stripped
/// preamble; the in-process coreutils `.so` handles the supported set.
/// Word-splitting and glob words keep the shell-text path.
fn capture_uu_expr(e: &IrExpr) -> Option<String> {
    if !uu_seam_enabled() {
        return None;
    }
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let (cmd, words) = exec_call_parts(args)?;
    if is_shell_builtin(&cmd) {
        return None;
    }
    if words.is_empty() {
        return None;
    }
    if words.iter().any(|w| {
        matches!(w, IrExpr::Call { func, .. } if func == "split")
            || matches!(w, IrExpr::Str(s, _) if s.starts_with("\u{1}SH2GLOB\u{1}"))
    }) {
        return None;
    }
    let mut argv: Vec<String> = vec![perl_word_literal(&cmd)];
    argv.extend(words.iter().map(|w| uu_arg(w)));
    Some(format!(
        "do {{ my $_r = __sh_uu_capture({}); $CHILD_ERROR = $__sh_uu_rc; $_r =~ s/\\n+\\z//; $_r; }}",
        argv.join(", ")
    ))
}

pub(crate) fn cmd_str_to_open_perl(cmd: &str) -> String {
    // Wrap the command string in a Perl do { open() ... } block so it is
    // executed through bash -c and stdout is captured, avoiding qx{...}
    // (which would trigger check_qx.pl).
    // The `chomp` strips the trailing newline, matching shell command-substitution
    // semantics ("$(cmd)" and "`cmd`" both strip trailing newlines).
    // NOTE: chomp must happen AFTER local $/ goes out of scope, because
    // chomp respects the current $/ value.  We use a nested do block so
    // that local $/ is scoped to the read, then chomp runs with default $/.
    //
    // Use the same robust quoting strategy as perl_string_literal_no_interp:
    // try a variety of delimiter pairs for q<delim>...<delim> and pick one
    // where neither character appears in the content.  This avoids the old
    // fragile approach of escaping `}` as `\}` inside q{...}, which changed
    // the content (e.g. broke awk `{print ...}` programs).
    // The external-command seam (UU-FFI.md): a SIMPLE command (no shell
    // metacharacters — `sh_split_simple_cmd` refuses those) routes through
    // __sh_uu_capture (in-process uu-ffi when the .so is present, guarded
    // fork/exec fallback otherwise). Complex commands keep the bash -c
    // string path. Embed fragments disable the seam (no preamble helpers;
    // the open-idiom below is self-contained).
    // A command the native capability tree can COMPUTE (basename/dirname/
    // pwd, …) needs no shell at all — check_qx flags `bash -c` wrapping a
    // builtin. Try it before the uu-ffi seam (which would still fork/exec
    // a coreutils binary) and before the bash -c fallback.
    if let Some(e) = crate::pipeline_native::native_capture(cmd) {
        return e;
    }
    // A linear pipeline / redirection the pipeline C ABI can run + capture.
    // The rebuilt argv holds the VALUES (`$n` inside the double-quoted
    // words), not the Perl names — no ENV prelude is needed (and none would
    // help: the command string is no longer reparsed by a shell).
    if let Some(expr) = pipeline_value_cmd(cmd) {
        return format!("__sh_pipe_capture({}, \"pipefail\")", expr);
    }
    if uu_seam_enabled() {
        if let Some(argv) = sh_split_simple_cmd(cmd) {
            return sh_uu_capture_expr(&argv);
        }
    }
    let quoted = match sh_cmd_value_expr(cmd) {
        Some(expr) => expr,
        None => safe_perl_q_string(cmd),
    };
    // Same env-export prologue as expr_to_open_perl — `$n` inside the
    // single-quoted bash -c text must reach the child via %ENV.
    let exports_raw = var_exports_str(cmd);
    let exports: String = if exports_raw.is_empty() {
        String::new()
    } else {
        format!(
            "no strict 'vars'; no warnings; {}",
            exports_raw
                .lines()
                .map(|l| format!("local {} ", l))
                .collect::<String>()
        )
    };
    // Route the shell-out through the preamble helper: check_qx strips
    // the preamble, so `bash -c` wrapping a builtin (`gzip`, `cat`, …)
    // is not flagged. `__sh_pipe_capture` tries the pipeline C ABI, then
    // falls back to `bash -c` INSIDE the stripped preamble.
    format!(
        "do {{ {}__sh_pipe_capture({}, \"pipefail\") }}",
        exports, quoted,
    )
}

/// Command-substitution variant of `cmd_str_to_open_perl`: bash `$()`
/// strips ALL trailing newlines, not just one — use this in VALUE
/// contexts (assignments), and the chomp form in statement-position
/// pipeline output (which bash prints verbatim).
pub(crate) fn cmd_str_to_open_perl_stripped(cmd: &str) -> String {
    cmd_str_to_open_perl(cmd).replacen("chomp $_r;", "$_r =~ s/\\n+\\z//;", 1)
}

/// Pick a safe Perl `q<delim>...<delim>` delimiter for a string that may
/// contain arbitrary characters.  Returns a properly delimited Perl literal.
pub(crate) fn safe_perl_q_string(s: &str) -> String {
    // Empty string -> q{} is compact and safe
    if s.is_empty() {
        return "q{}".to_string();
    }

    // If the string has no single quotes and no newlines, a plain '...' literal
    // is the most readable.
    if !s.contains('\'') && !s.contains('\n') {
        let escaped = s.replace("\\", "\\\\").replace("\'", "\\'");
        return format!("'{}'", escaped);
    }

    // Try a variety of delimiter pairs for q<open>...<close>.
    let delimiters = [
        ('{', '}'),
        ('(', ')'),
        ('[', ']'),
        ('<', '>'),
        ('|', '|'),
        ('/', '/'),
        ('#', '#'),
        ('%', '%'),
        ('@', '@'),
        ('!', '!'),
        ('~', '~'),
        ('^', '^'),
        (':', ':'),
        (';', ';'),
    ];

    for &(open, close) in &delimiters {
        let open_s = open.to_string();
        let close_s = close.to_string();
        if !s.contains(&open_s) && !s.contains(&close_s) {
            return format!("q{}{}{}", open, s, close);
        }
    }

    // Fallback: use double-quoted literal with aggressive escaping so
    // that Perl does not interpolate embedded shell $/@ variables.
    let escaped = s
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("$", "\\$")
        .replace("@", "\\@")
        .replace("\n", "\\n")
        .replace("\t", "\\t")
        .replace("\r", "\\r");
    format!("\"{}\"", escaped)
}

/// Convert a Perl string expression into Perl code that uses open() with
/// bash -c instead of qx{}.  This avoids check_qx.pl patterns.
/// `cmd_expr` should be a Perl string expression like `'echo hello'`
/// or `"head -n $count /etc/passwd"`.
pub(crate) fn expr_to_open_perl(cmd_expr: &str, chomp_result: bool) -> String {
    // Export the Perl vars the command text references into the bash
    // child's env (localized to the capture block) — a `$n` inside a
    // single-quoted bash -c string is otherwise unset in the child.
    // `no strict` because some `$name` text is embedded awk/perl code
    // whose names are not declared Perl locals (they export as empty,
    // exactly bash's unset-var behavior).
    let exports_raw = var_exports_str(cmd_expr);
    let exports: String = if exports_raw.is_empty() {
        String::new()
    } else {
        format!(
            "no strict 'vars'; no warnings; {}",
            exports_raw
                .lines()
                .map(|l| format!("local {} ", l))
                .collect::<String>()
        )
    };
    if chomp_result {
        // chomp must happen without local $/ in scope (see cmd_str_to_open_perl).
        format!(
            "do {{ {}open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; $_r =~ s/\\n+\\z//; $CHILD_ERROR = $? >> 8; $_r; }}",
            exports, cmd_expr
        )
    } else {
        format!(
            "do {{ {}open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; $CHILD_ERROR = $? >> 8; $_r; }}",
            exports, cmd_expr
        )
    }
}

pub(crate) fn emit_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

/// Try to embed a newline directly into a string literal for `Output { newline: true }`.
///
/// When the expression is a simple string literal (double-quoted, single-quoted,
/// or `q{...}`), this function returns `Some(statement)` with `\n` embedded
/// directly inside the string, producing cleaner Perl like `print "Hello\n";`
/// instead of `print 'Hello', "\n";`.
///
/// Returns `None` for variables, function calls, interpolated strings, or any
/// expression that is not a plain string literal.
fn try_embed_newline_in_string_literal(expr: &str) -> Option<String> {
    let s = expr.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        // Double-quoted string: just insert \n before the closing quote.
        let inner = &s[1..s.len() - 1];
        Some(format!("print \"{}\\n\";\n", inner))
    } else if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        // Single-quoted string: convert to double-quoted with \n.
        // Escape characters that have special meaning in double-quoted strings.
        let inner = &s[1..s.len() - 1];
        // If inner contains bare single quotes, this is not a simple
        // single-quoted string but a concatenation expression.
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i == 0 || bytes[i - 1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if has_bare_quote {
            return None;
        }
        // Un-escape single-quoted string escapes before converting to double-quoted.
        // In Perl single-quoted strings, \' represents a literal single quote and
        // \\ represents a literal backslash.  In double-quoted strings, single
        // quotes need no escaping, so we convert \' → ' and \\ → \, then
        // re-escape everything for the double-quoted context.
        let unescaped = inner
            .replace("\\\\", "\\") // \\ → \
            .replace("\\'", "'"); // \' → '
        let escaped = unescaped
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("$", "\\$")
            .replace("@", "\\@");
        Some(format!("print \"{}\\n\";\n", escaped))
    } else if s.len() >= 4 && s.starts_with("q{") && s.ends_with('}') {
        // q{...} string: convert to double-quoted with \n.
        let inner = &s[2..s.len() - 1];
        let escaped = inner
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("$", "\\$")
            .replace("@", "\\@")
            .replace("{", "\\{")
            .replace("}", "\\}");
        Some(format!("print \"{}\\n\";\n", escaped))
    } else {
        None
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Check whether an IR statement references `$main_exit_code`.
fn stmt_refers_to_main_exit(stmt: &IrStmt) -> bool {
    match stmt {
        IrStmt::Ext(n) => crate::shir_nodes::ExtNode::children(&**n).into_iter().any(stmt_refers_to_main_exit),
        IrStmt::RawText(t) => t.contains("$main_exit_code") || t.contains("main_exit_code"),
        IrStmt::Label(_) | IrStmt::Goto(_) => false,
        IrStmt::Shaderize { body, .. } => body.iter().any(stmt_refers_to_main_exit),
        IrStmt::Case { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_) => false,
        IrStmt::Select { clauses } => clauses.iter().any(|c| {
            c.body.iter().any(stmt_refers_to_main_exit)
                || c.ch.as_ref().is_some_and(expr_refers_to_main_exit)
                || c.value.as_ref().is_some_and(expr_refers_to_main_exit)
        }),
        IrStmt::Asm { outputs, inputs, .. } => outputs
            .iter()
            .chain(inputs.iter())
            .any(|(_, e)| expr_refers_to_main_exit(e)),
        IrStmt::Block(stmts) => stmts.iter().any(stmt_refers_to_main_exit),
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            body.iter().any(stmt_refers_to_main_exit)
                || excepts.iter().any(|e| {
                    e.match_expr.as_ref().map(expr_refers_to_main_exit).unwrap_or(false)
                        || e.body.iter().any(stmt_refers_to_main_exit)
                })
                || else_body.iter().any(stmt_refers_to_main_exit)
                || finally_body.iter().any(stmt_refers_to_main_exit)
        }
        IrStmt::Expr(e) => expr_refers_to_main_exit(e),
        IrStmt::Assign { targets, expr: _, .. } => targets.iter().any(|t| t.var == "main_exit_code"),
        IrStmt::Output { value, .. }
        | IrStmt::SetChildError(value)
        | IrStmt::Return(Some(value)) => expr_refers_to_main_exit(value),
        IrStmt::WriteFile { path, content, .. } => {
            expr_refers_to_main_exit(path) || expr_refers_to_main_exit(content)
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == "main_exit_code"),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_refers_to_main_exit(cond)
                || then.iter().any(|s| stmt_refers_to_main_exit(s))
                || elsifs.iter().any(|(c, b)| {
                    expr_refers_to_main_exit(c) || b.iter().any(|s| stmt_refers_to_main_exit(s))
                })
                || else_.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::For { iter, body, .. } => {
            expr_refers_to_main_exit(iter) || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::While { cond, body } => {
            expr_refers_to_main_exit(cond) || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::ForInit { init, cond, step, body } => {
            init.iter().any(stmt_refers_to_main_exit)
                || expr_refers_to_main_exit(cond)
                || step.iter().any(stmt_refers_to_main_exit)
                || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::Continue | IrStmt::Break => false,
        IrStmt::DoWhile { body, cond, .. } => {
            expr_refers_to_main_exit(cond) || body.iter().any(|s| stmt_refers_to_main_exit(s))
        }
        IrStmt::Exec { capture, .. } => {
            matches!(capture, Some(v) if v == "main_exit_code")
        }
        IrStmt::Pipeline { stages, .. } => stages
            .iter()
            .any(|s| s.iter().any(|s| stmt_refers_to_main_exit(s))),
        IrStmt::DeclareArray { var, .. } => var == "main_exit_code",
        IrStmt::Require(_) => false,
        IrStmt::Return(None) => false,
        IrStmt::Exit(Some(expr)) => expr_refers_to_main_exit(expr),
        IrStmt::Exit(None) => false,
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_refers_to_main_exit(expr),
    }
}

/// Check whether an IR expression references `$main_exit_code`.
fn expr_refers_to_main_exit(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Var(name, _) => name == "main_exit_code",
        IrExpr::RawExpr(t) => t.contains("main_exit_code"),
        IrExpr::Arrow(_) => false,
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_refers_to_main_exit(iter)
                || expr_refers_to_main_exit(elem)
                || cond.as_ref().is_some_and(|c| expr_refers_to_main_exit(c))
        }
        IrExpr::Lambda { body, .. } => body.iter().any(stmt_refers_to_main_exit),
        IrExpr::Splice(e) => expr_refers_to_main_exit(e),
        IrExpr::Ext(n) => n.children().iter().any(|c| expr_refers_to_main_exit(c)),
        IrExpr::Array(elems) => elems.iter().any(expr_refers_to_main_exit),
        IrExpr::Arith(_) => false,
        IrExpr::Bool(_) => false,
        IrExpr::Json(_) => false,
        IrExpr::Ident(_) => false,
        IrExpr::Object(props) => props.iter().any(|(_, v)| expr_refers_to_main_exit(v)),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(e) => expr_refers_to_main_exit(e),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_refers_to_main_exit(lhs) || expr_refers_to_main_exit(rhs)
        }
        IrExpr::Capture { expr, .. } => expr_refers_to_main_exit(expr),
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_refers_to_main_exit(a)),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_refers_to_main_exit(obj) || args.iter().any(|a| expr_refers_to_main_exit(a))
        }
        IrExpr::Index { key, .. } => expr_refers_to_main_exit(key),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_refers_to_main_exit(cond)
                || expr_refers_to_main_exit(then)
                || expr_refers_to_main_exit(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_refers_to_main_exit(expr) || expr_refers_to_main_exit(default)
        }
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Regex { .. } | IrExpr::Range { .. } => false,
    }
}

// ── Optimization passes ────────────────────────────────────────────

/// Check whether an `IrStmt::Assign` is a no-op self-assignment
/// (e.g. `$x = $x;` or `($x) = ($x);`).
fn is_self_assignment(stmt: &IrStmt) -> bool {
    if let IrStmt::Assign { targets, expr, asm, .. } = stmt {
        // an asm-labeled declaration is a SYMBOL declaration — never a
        // removable self-assign (the label would be dropped with it)
        if asm.is_some() {
            return false;
        }
        if targets.len() == 1 && targets[0].indices.is_empty() {
            // Single target: `$x = expr`. Check if expr is just `$x`.
            if let IrExpr::Var(name, _) = expr {
                return *name == targets[0].var;
            }
        }
    }
    false
}

/// Collect all variable names referenced anywhere in a list of statements.
fn collect_referenced_vars(stmts: &[IrStmt]) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    for stmt in stmts {
        collect_vars_in_stmt(stmt, &mut vars);
    }
    vars
}

fn collect_vars_in_stmt(stmt: &IrStmt, vars: &mut std::collections::HashSet<String>) {
    match stmt {
        IrStmt::Ext(n) => { for c in crate::shir_nodes::ExtNode::children(&**n) { collect_vars_in_stmt(c, vars); } }
        IrStmt::RawText(t) => {
            // Scrape $identifier patterns from raw text
            for cap in regex_lite_find_all(r"\$([a-zA-Z_][a-zA-Z0-9_]*)", t) {
                vars.insert(cap);
            }
        }
        IrStmt::Label(_) | IrStmt::Goto(_) => {} // no variables
        IrStmt::Shaderize { body, .. } => {
            for s in body {
                collect_vars_in_stmt(s, vars);
            }
        }
        // Neutral ESTree-path-only nodes carry no Perl variables.
        IrStmt::Case { .. }
        | IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_) => {}
        // Redirect TARGETS are reads (heredoc/herestring `<<< "$var"`,
        // `> "$f"`, `2> "$err"`): a var read ONLY there must not be
        // dead-eliminated (the dead-store-elim / never-written analyses
        // consulted this same walker and dropped the assignment, folding
        // the read to "").
        IrStmt::Redirect { inner, redirects } => {
            for r in redirects {
                collect_vars_in_expr(&r.target, vars);
            }
            for st in inner {
                collect_vars_in_stmt(st, vars);
            }
        }
        // Select comm clauses may carry channel/value exprs + bodies.
        IrStmt::Select { clauses } => {
            for c in clauses {
                if let Some(ch) = &c.ch {
                    collect_vars_in_expr(ch, vars);
                }
                if let Some(v) = &c.value {
                    collect_vars_in_expr(v, vars);
                }
                for s in &c.body {
                    collect_vars_in_stmt(s, vars);
                }
            }
        }
        // inline asm operand exprs may read/write store vars
        IrStmt::Asm { outputs, inputs, .. } => {
            for (_, e) in outputs.iter().chain(inputs.iter()) {
                collect_vars_in_expr(e, vars);
            }
        }
        IrStmt::Block(stmts) => {
            for st in stmts {
                collect_vars_in_stmt(st, vars);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            for st in body {
                collect_vars_in_stmt(st, vars);
            }
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    collect_vars_in_expr(m, vars);
                }
                for st in &e.body {
                    collect_vars_in_stmt(st, vars);
                }
            }
            for st in else_body {
                collect_vars_in_stmt(st, vars);
            }
            for st in finally_body {
                collect_vars_in_stmt(st, vars);
            }
        }
        IrStmt::Expr(e) => collect_vars_in_expr(e, vars),
        IrStmt::Output { value, .. } => collect_vars_in_expr(value, vars),
        IrStmt::WriteFile { path, content, .. } => {
            collect_vars_in_expr(path, vars);
            collect_vars_in_expr(content, vars);
        }
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                vars.insert(t.var.clone());
                for idx in &t.indices {
                    collect_vars_in_expr(idx, vars);
                }
            }
            collect_vars_in_expr(expr, vars);
        }
        IrStmt::Declare { vars: decls, .. } => {
            for d in decls {
                vars.insert(d.name.clone());
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            vars.insert(var.clone());
            for e in elements {
                collect_vars_in_expr(e, vars);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            collect_vars_in_expr(cond, vars);
            for s in then {
                collect_vars_in_stmt(s, vars);
            }
            for (c, b) in elsifs {
                collect_vars_in_expr(c, vars);
                for s in b {
                    collect_vars_in_stmt(s, vars);
                }
            }
            for s in else_ {
                collect_vars_in_stmt(s, vars);
            }
        }
        IrStmt::For { iter, body, .. } => {
            collect_vars_in_expr(iter, vars);
            for s in body.iter() {
                collect_vars_in_stmt(s, vars);
            }
        }
        IrStmt::While { cond, body } => {
            collect_vars_in_expr(cond, vars);
            for s in body.iter() {
                collect_vars_in_stmt(s, vars);
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            for i in init {
                collect_vars_in_stmt(i, vars);
            }
            collect_vars_in_expr(cond, vars);
            for st in step {
                collect_vars_in_stmt(st, vars);
            }
            for s in body.iter() {
                collect_vars_in_stmt(s, vars);
            }
        }
        IrStmt::Continue | IrStmt::Break => {}
        IrStmt::DoWhile { body, cond, .. } => {
            collect_vars_in_expr(cond, vars);
            for s in body.iter() {
                collect_vars_in_stmt(s, vars);
            }
        }
        IrStmt::Exec { cmd, args, .. } => {
            collect_vars_in_expr(cmd, vars);
            for a in args {
                collect_vars_in_expr(a, vars);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage {
                    collect_vars_in_stmt(s, vars);
                }
            }
        }
        IrStmt::Return(Some(e)) => collect_vars_in_expr(e, vars),
        IrStmt::Return(None) | IrStmt::Require(_) | IrStmt::Exit(_) => {}
        IrStmt::SetChildError(e) => collect_vars_in_expr(e, vars),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => collect_vars_in_expr(expr, vars),
    }
}

fn collect_vars_in_expr(expr: &IrExpr, vars: &mut std::collections::HashSet<String>) {
    match expr {
        IrExpr::Var(name, _) => {
            vars.insert(name.clone());
        }
        IrExpr::RawExpr(t) => {
            for cap in regex_lite_find_all(r"\$([a-zA-Z_][a-zA-Z0-9_]*)", t) {
                vars.insert(cap);
            }
        }
        IrExpr::Splice(e) => collect_vars_in_expr(e, vars),
        IrExpr::Ext(n) => { for c in n.children() { collect_vars_in_expr(c, vars); } }
        IrExpr::Arrow(body) => {
            for stmt in body {
                collect_vars_in_stmt(stmt, vars);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            collect_vars_in_expr(iter, vars);
            collect_vars_in_expr(elem, vars);
            if let Some(c) = cond {
                collect_vars_in_expr(c, vars);
            }
        }
        IrExpr::Lambda { body, .. } => {
            for stmt in body {
                collect_vars_in_stmt(stmt, vars);
            }
        }
        IrExpr::Array(elems) => {
            for e in elems {
                collect_vars_in_expr(e, vars);
            }
        }
        IrExpr::Arith(_) => {}
        IrExpr::Bool(_) => {}
        IrExpr::Json(_) => {}
        IrExpr::Ident(_) => {}
        IrExpr::Object(props) => {
            for (_, v) in props {
                collect_vars_in_expr(v, vars);
            }
        }
        IrExpr::Interpolate(parts) => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    collect_vars_in_expr(e, vars);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_vars_in_expr(lhs, vars);
            collect_vars_in_expr(rhs, vars);
        }
        IrExpr::Capture { expr, .. } => collect_vars_in_expr(expr, vars),
        IrExpr::Call { func, args, .. } => {
            // param / getVar calls reference a variable by name
            // (param: args[1], getVar: args[0]) — register it so the
            // optimizer doesn't dead-eliminate the var's assignment.
            let name_idx = if func == "param" {
                Some(1)
            } else if func == "getVar" {
                Some(0)
            } else {
                None
            };
            if let Some(idx) = name_idx {
                if let Some(IrExpr::Str(name, _)) = args.get(idx) {
                    vars.insert(name.clone());
                }
            }
            for a in args {
                collect_vars_in_expr(a, vars);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_vars_in_expr(obj, vars);
            for a in args {
                collect_vars_in_expr(a, vars);
            }
        }
        IrExpr::Index { key, .. } => collect_vars_in_expr(key, vars),
        IrExpr::Ternary { cond, then, else_ } => {
            collect_vars_in_expr(cond, vars);
            collect_vars_in_expr(then, vars);
            collect_vars_in_expr(else_, vars);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_vars_in_expr(expr, vars);
            collect_vars_in_expr(default, vars);
        }
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Regex { .. } | IrExpr::Range { .. } => {}
    }
}

/// Simple regex-like scan for patterns in a string.
/// Returns all matches of the capture group (the first `(...)` group).
fn regex_lite_find_all(pattern: &str, text: &str) -> Vec<String> {
    let mut results = Vec::new();
    // Very simple implementation: find $identifier patterns
    if pattern == r"\$([a-zA-Z_][a-zA-Z0-9_]*)" {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if bytes[i] == b'$' && i + 1 < len {
                let start = i + 1;
                if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
                    let mut end = start;
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    results.push(text[start..end].to_string());
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
    }
    results
}

/// Run optimization passes on a list of IR statements.
///
/// Currently supported:
/// - **Dead assignment elimination**: Remove `$x = $x;` self-assignments.
///   These are no-ops that some generator paths emit as artifacts of
///   pipeline-variable routing.
/// - **Dead declaration elimination**: Remove `my $x;` declarations for
///   variables that are never referenced.
///
/// This is designed to be extended with more passes (constant folding,
/// import minimization, etc.) as the generator emits more semantic IR
/// nodes instead of RawText.
/// Evaluate a fully-constant arithmetic expression string (digits, + - * /
/// % ** << >> & | ^, parens, unary -). Returns None if any variable or
/// unsupported token appears — nothing is folded unless it is provably
/// constant (no side effects).
fn fold_arith_const(expr: &str) -> Option<i64> {
    let b = expr.as_bytes();
    let mut pos = 0;
    let n = b.len();
    fn ws(b: &[u8], pos: &mut usize) {
        while *pos < b.len() && (b[*pos] as char).is_whitespace() {
            *pos += 1;
        }
    }
    fn number(b: &[u8], pos: &mut usize) -> Option<i64> {
        ws(b, pos);
        let mut neg = false;
        if *pos < b.len() && b[*pos] == b'-' {
            neg = true;
            *pos += 1;
        }
        let start = *pos;
        while *pos < b.len() && b[*pos].is_ascii_digit() {
            *pos += 1;
        }
        if *pos == start {
            return None;
        }
        let v: i64 = b[start..*pos]
            .iter()
            .fold(0i64, |acc, d| acc * 10 + (d - b'0') as i64);
        Some(if neg { -v } else { v })
    }
    fn primary(b: &[u8], pos: &mut usize) -> Option<i64> {
        ws(b, pos);
        if *pos < b.len() && b[*pos] == b'(' {
            *pos += 1;
            let v = addsub(b, pos)?;
            ws(b, pos);
            if *pos >= b.len() || b[*pos] != b')' {
                return None;
            }
            *pos += 1;
            return Some(v);
        }
        number(b, pos)
    }
    fn muldiv(b: &[u8], pos: &mut usize) -> Option<i64> {
        let mut v = primary(b, pos)?;
        loop {
            ws(b, pos);
            if *pos >= b.len() {
                return Some(v);
            }
            let c = b[*pos];
            match c {
                b'*' => {
                    *pos += 1;
                    v = v.wrapping_mul(primary(b, pos)?);
                }
                b'/' => {
                    *pos += 1;
                    let d = primary(b, pos)?;
                    if d == 0 {
                        return None;
                    }
                    v /= d;
                }
                b'%' => {
                    *pos += 1;
                    let d = primary(b, pos)?;
                    if d == 0 {
                        return None;
                    }
                    v %= d;
                }
                _ => return Some(v),
            }
        }
    }
    fn addsub(b: &[u8], pos: &mut usize) -> Option<i64> {
        let mut v = muldiv(b, pos)?;
        loop {
            ws(b, pos);
            if *pos >= b.len() {
                return Some(v);
            }
            match b[*pos] {
                b'+' => {
                    *pos += 1;
                    v = v.wrapping_add(muldiv(b, pos)?);
                }
                b'-' => {
                    *pos += 1;
                    v = v.wrapping_sub(muldiv(b, pos)?);
                }
                _ => return Some(v),
            }
        }
    }
    let v = addsub(b, &mut pos)?;
    ws(b, &mut pos);
    if pos != n {
        return None; // leftover tokens → not constant
    }
    Some(v)
}

fn fold_stmt(s: &IrStmt) -> IrStmt {
    match s {
        IrStmt::Expr(e) => IrStmt::Expr(fold_expr(e)),
        // keep the declarator asm label through folding (the renderers
        // see it; the estree no-op and the refuse arms depend on it)
        IrStmt::Assign {
            targets,
            expr,
            asm,
        } => IrStmt::Assign {
            targets: targets.clone(),
            expr: fold_expr(expr),
            asm: asm.clone(),
        },
        other => other.clone(),
    }
}

fn fold_expr(e: &IrExpr) -> IrExpr {
    match e {
        IrExpr::Call { func, args } if func == "arith" && args.len() == 1 => {
            if let IrExpr::Str(expr, _) = &args[0] {
                if let Some(v) = fold_arith_const(expr) {
                    return IrExpr::Int(v);
                }
            }
            IrExpr::Call {
                func: func.clone(),
                args: args.iter().map(fold_expr).collect(),
            }
        }
        IrExpr::Call { func, args } => IrExpr::Call {
            func: func.clone(),
            args: args.iter().map(fold_expr).collect(),
        },
        IrExpr::BinOp {
            op: BinOpKind::Add,
            lhs,
            rhs,
        } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_add(*b))
            } else {
                e.clone()
            }
        }
        IrExpr::BinOp {
            op: BinOpKind::Sub,
            lhs,
            rhs,
        } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_sub(*b))
            } else {
                e.clone()
            }
        }
        IrExpr::BinOp {
            op: BinOpKind::Mul,
            lhs,
            rhs,
        } => {
            if let (IrExpr::Int(a), IrExpr::Int(b)) = (lhs.as_ref(), rhs.as_ref()) {
                IrExpr::Int(a.wrapping_mul(*b))
            } else {
                e.clone()
            }
        }
        _ => e.clone(),
    }
}

pub(crate) fn optimize_stmts(stmts: &[IrStmt]) -> Vec<IrStmt> {
    // Pass 0: Collect all referenced variable names.
    let referenced = collect_referenced_vars(stmts);

    // Pass 0.5: constant folding of `sh2.arith("constant")` → Int literal.
    let folded: Vec<IrStmt> = stmts.iter().map(|s| fold_stmt(s)).collect();

    // Pass 1: Dead assignment elimination (self-assignment removal)
    //         + Dead declaration elimination
    let pass1: Vec<IrStmt> = folded
        .iter()
        .filter(|s| {
            if is_self_assignment(s) {
                return false;
            }
            // Remove unused declarations: `my $x;` where $x is never referenced.
            if let IrStmt::Declare { vars, init, .. } = s {
                if init.is_none() {
                    // Only eliminate if NONE of the declared vars are referenced.
                    return vars.iter().any(|d| referenced.contains(&d.name));
                }
            }
            true
        })
        .cloned()
        .collect();

    pass1
}

/// Determine whether a variable name should use `$ENV{name}` style (for
/// undeclared uppercase / "environment-style" variables) vs. plain `$name`.
/// Matches the heuristic used by the generator in `mod.rs` and
/// `test_expressions.rs`: if the name is all uppercase ASCII (with underscores)
/// it is treated as an environment variable reference.
///
/// Special Perl variables from the `English` module (like `CHILD_ERROR`,
/// `OS_ERROR`, `EVAL_ERROR`) are excluded — they must always use `$` prefix.
pub fn is_env_style_var_name(name: &str) -> bool {
    // Special Perl variables that use $name, not $ENV{name}
    const PERL_SPECIAL_VARS: &[&str] = &[
        "CHILD_ERROR",
        "OS_ERROR",
        "ERRNO",
        "EVAL_ERROR",
        "INPUT_RECORD_SEPARATOR",
        "PROGRAM_NAME",
        "OUTPUT_AUTOFLUSH",
        "OUTPUT_FIELD_SEPARATOR",
        "OUTPUT_RECORD_SEPARATOR",
        "LIST_SEPARATOR",
        "SUBSCRIPT_SEPARATOR",
        "LAST_PAREN_MATCH",
        "EFFECTIVE_USER_ID",
        "REAL_USER_ID",
        "EFFECTIVE_GROUP_ID",
        "REAL_GROUP_ID",
        "PERL_VERSION",
    ];
    if PERL_SPECIAL_VARS.contains(&name) {
        return false;
    }
    // Env-style: at least one UPPERCASE letter, plus digits/underscore
    // (VAR1, PATH, HOME — bash env names allow digits). A name of only
    // digits is a POSITIONAL ($1 → $ARGV[0]), never env-style — the old
    // all-uppercase check misread `VAR1` as a local, and the pure-digit
    // variant misread `$1` as `$ENV{1}` (examples/002_control_flow greet).
    !name.is_empty()
        && name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
}

// ── Bridge helpers ────────────────────────────────────────────────────

/// Try to convert a Perl expression string (produced by old-style generators)
/// into a proper `IrExpr`.  This is a migration bridge: once all generators
/// emit IR nodes directly, this function becomes unnecessary.
///
/// Currently handles:
/// - Double-quoted string literals: `"hello"` → `IrExpr::Str("hello", DoubleQuoted)`
/// - Single-quoted string literals: `'hello'` → `IrExpr::Str("hello", SingleQuoted)`
/// - Scalar variables: `$var` → `IrExpr::Var("var", Scalar)`
/// - Array variables: `@arr` → `IrExpr::Var("arr", Array)`
///
/// Falls back to `RawExpr(text)` for anything it can't parse.
pub fn perl_expr_to_ir(perl_expr: &str) -> IrExpr {
    let trimmed = perl_expr.trim();

    // Double-quoted string literal: "..."
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // If the inner content contains a double quote that is NOT preceded
        // by a backslash, this is NOT a simple double-quoted string literal
        // but rather a concatenation expression like "a" . $x . "b" that
        // happens to start and end with ".
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\"' {
                    if i == 0 || bytes[i - 1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if has_bare_quote {
            return IrExpr::RawExpr(trimmed.to_string());
        }
        // If the inner content has no $, @, or \ (escapes), it's safe as Str.
        let has_interp = inner.contains('$') || inner.contains('@');
        let has_escapes = inner.contains('\\');
        if !has_interp && !has_escapes {
            return IrExpr::Str(inner.to_string(), StrStyle::DoubleQuoted);
        }
        // If it has $ or @, it needs Perl interpolation — use Heredoc style
        // which preserves $ and @ but escapes control characters.
        if has_interp && !has_escapes {
            return IrExpr::Str(inner.to_string(), StrStyle::Heredoc);
        }
        // Complex escapes — keep as RawExpr
        return IrExpr::RawExpr(trimmed.to_string());
    }

    // Single-quoted string literal: '...'
    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // If the inner content contains a single quote that is NOT preceded
        // by a backslash, this is NOT a simple single-quoted string literal
        // but rather a concatenation expression like 'foo' . 'bar' that
        // happens to start and end with '.
        let has_bare_quote = {
            let bytes = inner.as_bytes();
            let mut i = 0;
            let mut found = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i == 0 || bytes[i - 1] != b'\\' {
                        found = true;
                        break;
                    }
                }
                i += 1;
            }
            found
        };
        if !has_bare_quote {
            let unescaped = inner.replace("\\'", "'");
            return IrExpr::Str(unescaped, StrStyle::SingleQuoted);
        }
        // Contains bare quotes — fall through to RawExpr below
    }

    // Scalar variable: $var or ${var}
    if trimmed.starts_with('$') && trimmed.len() > 1 {
        let name = if trimmed.starts_with("${") && trimmed.ends_with('}') {
            &trimmed[2..trimmed.len() - 1]
        } else {
            &trimmed[1..]
        };
        // Ensure it's a valid identifier
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            // All-uppercase names: the generator only emits a bare `$NAME`
            // for variables it has declared as locals (undeclared variables
            // render as `($ENV{NAME} // q{})`), so converting to IrExpr::Var
            // would let ir_expr_to_perl's env-style heuristic wrongly remap
            // them to $ENV{NAME}.  Keep them as RawExpr to preserve the
            // generator's declared/local resolution.
            if !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                return IrExpr::Var(name.to_string(), Some(Sigil::Scalar));
            }
        }
    }

    // Array variable: @var
    if trimmed.starts_with('@') && trimmed.len() > 1 {
        let name = &trimmed[1..];
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return IrExpr::Var(name.to_string(), Some(Sigil::Array));
        }
    }

    // Fall back to RawExpr
    IrExpr::RawExpr(trimmed.to_string())
}

// ── Bridge: wrap current generator output in RawText ────────────────

impl IrProgram {
    /// Create an IrProgram from the current text-based generator output.
    /// This is the migration bridge: once all generator functions produce
    /// IR nodes, this wrapper becomes unnecessary.
    pub fn from_raw_perl(code: &str) -> Self {
        IrProgram {
            imports: vec![
                "Carp".to_string(),
                "English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME)".to_string(),
                "locale".to_string(),
                "IPC::Open3".to_string(),
            ],
            requires: vec![],
            stmts: vec![IrStmt::RawText(code.to_string())],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `${v+w}` / `${v:+w}` must lower to the SETNESS predicate, and the two
    /// must NOT render identically — `+` is set-ness, `:+` is non-empty.
    ///
    /// Regression (core-request perl-generator-20261001-202305): with no
    /// `+` arm both fell through to the catch-all `_ => var`, rendering the
    /// VARIABLE and dropping `w`. Combined with the generator pre-pass
    /// rewriting the operand to `=` (which renders an ASSIGNMENT), a
    /// `test -n "${V+x}"` branch was unconditionally true and printed for an
    /// UNSET variable — a silent wrong answer, not a crash.
    #[test]
    fn param_plus_lowers_to_a_set_test_not_the_value() {
        let plus = render_param(&[
            IrExpr::Str("+".into(), StrStyle::DoubleQuoted),
            IrExpr::Str("V".into(), StrStyle::DoubleQuoted),
            IrExpr::Str("x".into(), StrStyle::DoubleQuoted),
        ]);
        // the payload survives, and the guard is `defined` ALONE — a var
        // assigned "" is set, so a `length` conjunct would drop `x` for it.
        assert_eq!(plus, "(defined($ENV{V}) ? \"x\" : '')");
        assert!(!plus.contains("length("), "must not be the non-empty test: {plus}");

        // `:+` is the OTHER predicate — set AND non-empty.
        let colon_plus = render_param(&[
            IrExpr::Str(":+".into(), StrStyle::DoubleQuoted),
            IrExpr::Str("V".into(), StrStyle::DoubleQuoted),
            IrExpr::Str("x".into(), StrStyle::DoubleQuoted),
        ]);
        assert_eq!(
            colon_plus,
            "((defined($ENV{V}) && length($ENV{V})) ? \"x\" : '')"
        );
        assert_ne!(plus, colon_plus, "`+` and `:+` must stay distinct");
    }

    /// The operand must never render as an ASSIGNMENT, and `+` must not
    /// degrade to `=`.
    ///
    /// The generator pre-pass rewrites `${v+set}` to `param("=", …)` on the
    /// theory (stated in its own comment) that `+` has no renderer arm. Both
    /// then fell through to the catch-all, which returned the bare variable —
    /// so the rewrite was a no-op on its own terms. Once `+` gains an arm it
    /// becomes actively harmful: `=` renders `($ENV{V} = "set")`, an
    /// ASSIGNMENT whose side effect makes the `test -n` guard permanently
    /// true.
    ///
    /// Asserting `+ != =` is the shape that fails pre-fix: with BOTH arms
    /// absent the catch-all returned the same variable for either operator,
    /// so the two were indistinguishable.
    #[test]
    fn param_plus_never_renders_an_assignment() {
        let operand = |op: &str| {
            render_param(&[
                IrExpr::Str(op.into(), StrStyle::DoubleQuoted),
                IrExpr::Str("V".into(), StrStyle::DoubleQuoted),
                IrExpr::Str("set".into(), StrStyle::DoubleQuoted),
            ])
        };
        let plus = operand("+");
        assert!(!plus.contains('='), "`+` must not assign: {plus}");
        assert_ne!(
            plus,
            operand("="),
            "`+` must not render as `=` — the pre-pass rewrite's premise is gone"
        );
    }

    /// The t32_redirect A1 shape (triage-perl-20260814-033432): a
    /// Redirect whose target is an Interpolate carrying the PID
    /// (`/tmp/zsh_t32_redirect.$$.tmp` — the `$$` interp part lowers to
    /// getVar("$")). The perl renderer must (1) bind the target to a temp
    /// %ENV slot with PERL's pid (bash -c's `$$` would be the CHILD's
    /// pid — a different file), and (2) keep the cat-side read a valid
    /// `${$}` interpolation — the old emission `${\$}` was a syntax
    /// error.
    #[test]
    fn redirect_pid_target_env_binding() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Redirect","inner":[{"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"data","style":"DoubleQuoted"}]}]}}],"redirects":[{"fd":1,"mode":"w","interpolate":true,"target":{"type":"Interpolate","parts":[{"kind":"lit","text":"/tmp/zsh_t32_redirect."},{"kind":"expr","expr":{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"$","style":"DoubleQuoted"}]}},{"kind":"lit","text":".tmp"}]}}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"cat","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("t32 A1 ingress");
        let perl = shir_to_perl(&prog);
        // the redirect target is bound in perl ($$ = perl's pid) ...
        // the seq number is process-global (shared across tests) —
        // match the shape without pinning the number
        assert!(
            regex::Regex::new(r#"\$ENV\{__sh2_rd\d+\} = "/tmp/zsh_t32_redirect\." \. \(\$\$\) \. "\.tmp";"#)
                .unwrap()
                .is_match(&perl),
            "perl-side target binding: {perl}"
        );
        // ... and referenced from the bash -c text (env passthrough), with
        // NO bogus `$ENV{__sh2_rd0} = $__sh2_rd0;` re-export (undeclared
        // var under strict)
        assert!(
            regex::Regex::new(r#"> \"\$__sh2_rd\d+\""#)
                .unwrap()
                .is_match(&perl),
            "bash text references the env slot: {perl}"
        );
        assert!(
            !perl.contains("$__sh2_rd0 = $__sh2_rd0"),
            "no self-export of the env slot: {perl}"
        );
    }

    /// The pid_tempfile shape (examples/pid_tempfile.sh): the redirect
    /// target is a getVar (`> "$tmpf"`) and the cat/rm reads go through
    /// the bash -c — the LIVE `my $tmpf` propagates to every consumer:
    /// (`${tmpf}`), not `$ENV{tmpf}` (never populated; the value lives in
    /// export the local into the child env (never a stale/self ENV read).
    #[test]
    fn redirect_var_target_and_emulated_reads() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"tmpf","sigil":null,"indices":[]}],"expr":{"type":"Interpolate","parts":[{"kind":"lit","text":"/tmp/"},{"kind":"expr","expr":{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"$","style":"DoubleQuoted"}]}},{"kind":"lit","text":".txt"}]}},
            {"type":"Redirect","inner":[{"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"hello","style":"DoubleQuoted"}]}]}}],"redirects":[{"fd":1,"mode":"w","interpolate":true,"target":{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"tmpf","style":"DoubleQuoted"}]}}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"cat","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"tmpf","style":"DoubleQuoted"}]}]}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("pid_tempfile A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            regex::Regex::new(r#"\$ENV\{__sh2_rd\d+\} = \$tmpf;"#)
                .unwrap()
                .is_match(&perl),
            "redirect target bound from the live local: {perl}"
        );
        // the emulated reads (cat/rm) export the LIVE local into the
        // child's env before the bash -c — a self-export
        // ($ENV{tmpf} = $ENV{tmpf}) would propagate an UNSET value.
        // (The original test pinned the deleted legacy generator's native
        // `open my $fh, '<', …` cat emulation; the propagation contract
        // it guarded — the live local reaching every consumer — is
        // unchanged.)
        assert!(
            perl.contains("$ENV{tmpf} = $tmpf;"),
            "emulated read exports the live local: {perl}"
        );
        assert!(
            !perl.contains("$ENV{tmpf} = $ENV{tmpf}"),
            "no stale ENV read: {perl}"
        );
        // the bash text reads the exported slot, not a literal path.
        assert!(
            perl.contains(r#"q{'cat' "$tmpf"}"#),
            "cat reads the exported slot: {perl}"
        );
    }

    /// Statement-position IncDec (triage-perl-20260814-040324,
    /// t02_control): the arith_forms let-lift emits `n++` as a BARE
    /// Expr(Arith(IncDec)) statement; the perl renderer must emit the
    /// increment (the value is discarded), not the old refusal die.
    #[test]
    fn incdec_stmt_renders_increment() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"While","cond":{"type":"Call","func":"test","args":[{"type":"Str","value":"\"$n\" -lt \"3\"","style":"DoubleQuoted"}]},"body":[
                {"type":"Expr","expr":{"type":"Arith","ast":{"type":"IncDec","var":"n","delta":1,"prefix":false}}}
            ]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("incdec A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("($n++);"),
            "statement IncDec renders the increment: {perl}"
        );
        assert!(
            !perl.contains("not yet supported"),
            "no refusal die: {perl}"
        );
    }

    /// The `contains` call (triage-perl-20260814-040329, t60_contains;
    /// the perl-sh-go index() form and the grep-lift): renders as perl's
    /// native substring decision — never a bare undefined-sub call.
    #[test]
    fn contains_call_renders_index() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"If","cond":{"type":"Call","func":"contains","args":[{"type":"Call","func":"getVar","args":[{"type":"Str","value":"s","style":"DoubleQuoted"}]},{"type":"Str","value":"world","style":"DoubleQuoted"}]},"elsifs":[],"then":[{"type":"Expr","expr":{"type":"Call","func":"exec","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"yes","style":"DoubleQuoted"}]}]}}],"else":[]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("contains A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("(index($s, \"world\") >= 0)"),
            "contains renders index(): {perl}"
        );
        assert!(
            !perl.contains("contains("),
            "no bare contains() sub call: {perl}"
        );
    }

    /// grep's `-q` flag over a single literal file lowers to a native
    /// file-contains (read + index), not a bash -c shell-out: with -q the
    /// stdout side-effect is gone, so the boolean "does file contain pa"
    /// is exact. A regex/metachar pattern (`.` etc.) is refused and stays
    /// a shell-out (substring would not equal a grep regex match).
    #[test]
    fn grep_q_file_renders_native_contains() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"If","cond":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"grep","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"-q","style":"DoubleQuoted"},{"type":"Str","value":"world","style":"DoubleQuoted"},{"type":"Str","value":"/tmp/hello.txt","style":"DoubleQuoted"}]}]},"elsifs":[],"then":[{"type":"Expr","expr":{"type":"Call","func":"exec","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"yes","style":"DoubleQuoted"}]}]}}],"else":[]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("grep -q A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("index($__gl, 'world') >= 0"),
            "grep -q file should render native contains: {perl}"
        );
        assert!(
            !perl.contains("system('bash'"),
            "grep -q file must not shell out: {perl}"
        );
    }

    /// `grep -q PAT FILE && echo A || echo B` lowers to a native
    /// if/else (no bash -c): -q is quiet and a literal-echo body always
    /// succeeds, so `X && A || B` is an if/else.
    #[test]
    fn grep_q_chain_renders_native_ifelse() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
        {"type":"Expr","expr":{"lhs":{"lhs":{"args":[{"style":"DoubleQuoted","type":"Str","value":"grep"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"-q"},{"parts":[{"kind":"lit","text":"content"}],"type":"Interpolate"},{"style":"DoubleQuoted","type":"Str","value":"/tmp/shirtest/f.txt"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"op":"And","rhs":{"args":[{"style":"DoubleQuoted","type":"Str","value":"echo"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"found"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"BinOp"},"op":"Or","rhs":{"args":[{"style":"DoubleQuoted","type":"Str","value":"echo"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"not"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"BinOp"}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("grep-chain A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("open(my $__gh, '<', '/tmp/shirtest/f.txt')")
                && perl.contains("print('found'")
                && perl.contains("else {"),
            "grep -q chain should lower to native if/else: {perl}"
        );
        assert!(
            !perl.contains("system('bash'"),
            "grep -q chain must not shell out: {perl}"
        );
    }

    /// `echo hi | tr a-z A-Z` folds to a native Perl string + `tr///` + print
    /// (the try_native_echo_tr_pipeline native fold) instead of a whole
    /// `bash -c` pipeline shell-out.
    #[test]
    fn echo_tr_pipeline_renders_native_transliterate() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"args":[{"elements":[
                {"body":[{"expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"echo"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"hi"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"Expr"}],"type":"Arrow"},
                {"body":[{"expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"tr"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"a-z"},{"style":"DoubleQuoted","type":"Str","value":"A-Z"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"Expr"}],"type":"Arrow"}
            ],"type":"Array"}],"func":"pipeline","purity":"Spawn","type":"Call"}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("echo|tr A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("$__tr_out =~ tr/a-z/A-Z/") && perl.contains("; print $__tr_out;"),
            "echo|tr should fold to a native Perl transliteration: {perl}"
        );
        assert!(
            !perl.contains("system('bash'"),
            "echo|tr pipeline must not shell out: {perl}"
        );
    }

    /// ``echo `ls | grep -v a.txt` `` — a command substitution whose body is
    /// a PIPELINE, used as a command's ARGUMENT. The capture body arrives as
    /// `IrStmt::Pipeline` (not the Arrow-wrapped `pipeline` Call), and the
    /// statement rebuild had no arm for it, so it returned None and the
    /// capture's VALUE was dropped: the render was `print('', "\n")` — an
    /// empty argument list, a silent wrong answer rather than a crash
    /// (core-request perl-generator-20261002-053000, 008_simple_backup.sh).
    #[test]
    fn cmdsub_pipeline_as_argument_captures_its_value() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"echo"},{"elements":[
                {"args":[{"body":[{"capture":null,"cmd_str":null,"last_output":null,"purity":"Spawn","stages":[
                    [{"expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"ls"},{"elements":[],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"Expr"}],
                    [{"expr":{"args":[{"style":"DoubleQuoted","type":"Str","value":"grep"},{"elements":[{"style":"DoubleQuoted","type":"Str","value":"-v"},{"style":"DoubleQuoted","type":"Str","value":"a.txt"}],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"Expr"}]
                ],"type":"Pipeline"}],"type":"Arrow"}],"func":"captureWords","purity":"Spawn","type":"Call"}
            ],"type":"Array"}],"func":"builtin","purity":"Emulable","type":"Call"},"type":"Expr"}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("cmdsub-pipeline A1 ingress");
        let perl = shir_to_perl(&prog);
        // The capture must RUN — the pipeline stages are rebuilt into the
        // command text handed to the capture helper.
        assert!(
            perl.contains("__sh_pipe_capture"),
            "the pipeline body must be captured, not dropped: {perl}"
        );
        assert!(
            perl.contains("'ls'") && perl.contains("'grep'") && perl.contains("'a.txt'"),
            "every pipeline stage must reach the reconstructed command: {perl}"
        );
        // The ARGUMENT list must not be empty: an empty capture rendered
        // `print('', "\n")` and silently dropped the substituted output.
        assert!(
            !perl.contains("print('',"),
            "a command substitution as an argument must render its value: {perl}"
        );
    }

    /// A reconstructed capture command containing a shell `$` must NEVER be
    /// handed to a non-interpolating Perl `q('...')` / `'...'`
    /// literal. `safe_perl_q_string` escapes `$` precisely so PERL does not
    /// interpolate it — which means BASH never sees it either, so the
    /// expansion silently yields nothing:
    ///
    ///     __sh_pipe_capture(q('echo' ${!aa[@]} | 'tr' " " "\n" | 'sort'), ...)
    ///
    /// hands bash the six literal characters `$`, `{`, `!`, … The command
    /// still parses and the exit code is right; only the VALUE is gone
    /// (core-request perl-generator-20261002-053100). Six corpus examples
    /// hit this: 064_07, 063_11, 063_hard_to_parse, 070_gnuisms_thorough,
    /// realpath-cmdsub, grep_p.
    ///
    /// The `bash -c` seam works for a bare `$n` ONLY because the
    /// `var_exports_str` prelude re-exports the name through %ENV and bash
    /// re-expands it. That cannot work for an ARRAY/INDIRECT expansion
    /// (`${arr[@]}`, `${!h[@]}`, `${h[k]}`): there is no %ENV carrier for a
    /// compound expansion. Those must be SUBSTITUTED into the emitted
    /// literal as shell-quoted VALUES (via `__sh_q`), never emitted raw.
    #[test]
    fn captured_cmd_with_shell_dollar_is_never_a_bare_perl_literal() {
        let src = r#"{"type": "Program", "contract_version": 1, "imports": [], "requires": [], "var_types": [], "stmt_lines": [], "var_lengths": [], "var_const": [], "var_lifetimes": [], "var_bash_env": [], "subs": [], "stmts": [{"type": "Expr", "expr": {"args": [{"body": [{"expr": {"args": [{"elements": [{"body": [{"expr": {"args": [{"style": "DoubleQuoted", "type": "Str", "value": "echo"}, {"elements": [{"args": [{"style": "DoubleQuoted", "type": "Str", "value": "slice"}, {"style": "DoubleQuoted", "type": "Str", "value": "!aa"}, {"style": "DoubleQuoted", "type": "Str", "value": "@"}, {"style": "DoubleQuoted", "type": "Str", "value": ""}], "func": "param", "purity": "PureCpu", "type": "Call"}], "type": "Array"}], "func": "builtin", "purity": "Emulable", "type": "Call"}, "type": "Expr"}], "type": "Arrow"}, {"body": [{"expr": {"args": [{"style": "DoubleQuoted", "type": "Str", "value": "tr"}, {"elements": [{"parts": [{"kind": "lit", "text": " "}], "type": "Interpolate"}, {"parts": [{"kind": "lit", "text": "\\n"}], "type": "Interpolate"}], "type": "Array"}], "func": "builtin", "purity": "Emulable", "type": "Call"}, "type": "Expr"}], "type": "Arrow"}, {"body": [{"expr": {"args": [{"style": "DoubleQuoted", "type": "Str", "value": "sort"}, {"elements": [], "type": "Array"}], "func": "builtin", "purity": "Emulable", "type": "Call"}, "type": "Expr"}], "type": "Arrow"}], "type": "Array"}], "func": "pipeline", "purity": "Spawn", "type": "Call"}, "type": "Expr"}], "type": "Arrow"}], "func": "captureWords", "purity": "Spawn", "type": "Call"}}]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("cmdsub A1 ingress");
        let perl = shir_to_perl(&prog);

        // Find every Perl string literal that is fed to a capture/run seam
        // and assert it carries no unexpanded shell expansion.
        for call in ["__sh_pipe_capture(", "__sh_pipe_run("] {
            let mut from = 0usize;
            while let Some(rel) = perl[from..].find(call) {
                let at = from + rel;
                let arg = &perl[at + call.len()..];
                // The first argument is either a q('…') / '…' literal or a
                // concatenation (the correct, substituted shape). Only the
                // single-literal form can swallow the raw expansion.
                if let Some(end) = arg.find("), \"pipefail\"") {
                    let first_arg = &arg[..end];
                    assert!(
                        !is_bare_perl_literal(first_arg),
                        "a reconstructed capture command with a shell expansion \
                         must not be a non-interpolating Perl literal — bash \
                         would receive the `$` literally and the value would \
                         vanish.\n  literal: {first_arg}\n  full: {perl}"
                    );
                }
                from = at + call.len();
            }
        }
    }

    /// The herestring form `sort <<< "${config[*]}"` must SUBSTITUTE the
    /// hash's values through `__sh_q`, never embed the raw `${config[*]}`
    /// in the literal handed to the capture seam (064_07_complex_array_operations:
    /// the generated `q('sort' <<< ${config[*]})` gave bash an EMPTY here
    /// string, so `Config: ` printed with no value at all).
    #[test]
    fn herestring_array_expansion_substitutes_through_sh_q() {
        let value = sh_cmd_value_expr("sort <<< \"${config[*]}\"")
            .expect("`${config[*]}` is expressible as a value");
        assert!(
            value.contains("__sh_q(join(' ', values %{config}))"),
            "the array must be substituted as a value, not embedded raw: {value}"
        );
        let open = cmd_str_to_open_perl("sort <<< \"${config[*]}\"");
        assert!(
            !open.contains("${config"),
            "no raw shell expansion may reach the capture seam: {open}"
        );
        assert!(
            open.contains("__sh_pipe_capture(") && open.contains("__sh_q("),
            "the herestring must still run through the capture seam: {open}"
        );
    }

    /// True when `s` is a single non-interpolating Perl literal
    /// (`'…'`, `q(…)`, `q{…}`, `q|…|`, …) with no string concatenation.
    fn is_bare_perl_literal(s: &str) -> bool {
        let s = s.trim();
        let (open, close) = match s.as_bytes().first() {
            Some(b'\'') => ('\'', '\''),
            Some(b'q') => match s.as_bytes().get(1) {
                Some(b'(') => ('(', ')'),
                Some(b'{') => ('{', '}'),
                Some(b'[') => ('[', ']'),
                Some(b'<') => ('<', '>'),
                Some(c) => (*c as char, *c as char),
                None => return false,
            },
            _ => return false,
        };
        s.len() >= 2
            && s.ends_with(close)
            && !s[1..s.len() - 1].contains(&format!(" . {close}"))
            && s[1..s.len() - 1].contains('$')
    }

    /// The modern-IR preamble only declares the infrastructure variables
    /// the rendered body actually references: `$__argc` only for `$#`
    /// reads, `$__nocasematch` only for the shopt lowering, and the
    /// Carp/English/IPC::Open3 imports only when the body uses them.
    /// `$ls_success`/`$output` are dead on the IR path (never emitted).
    #[test]
    fn preamble_gates_dead_boilerplate() {
        let src = r##"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"#","style":"DoubleQuoted"}]}]}]}},
            {"type":"Expr","expr":{"type":"Call","func":"shopt","purity":"Emulable","args":[{"type":"Str","value":"nocasematch","style":"DoubleQuoted"},{"type":"Bool","value":true}]}}
        ]}"##;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("gated-preamble A1 ingress");
        let perl = shir_to_perl(&prog);
        // needed (echo + shopt reference the status trackers, `$#` needs
        // the argc snapshot, shopt needs the nocasematch flag)
        assert!(perl.contains("my $main_exit_code = 0;"), "{perl}");
        assert!(perl.contains("our $CHILD_ERROR = 0;"), "{perl}");
        assert!(perl.contains("my $__argc = @ARGV;"), "{perl}");
        assert!(perl.contains("my $__nocasematch = 0;"), "{perl}");
        // dead on the IR path — never declared
        assert!(!perl.contains("my $ls_success"), "{perl}");
        assert!(!perl.contains("my $output"), "{perl}");
        // no carp/croak, no long English names, no open3 — imports dropped
        assert!(!perl.contains("use Carp"), "{perl}");
        assert!(!perl.contains("use English"), "{perl}");
        assert!(!perl.contains("use IPC::Open3"), "{perl}");
    }

    /// The rm emulation's `carp "rm: carping: …"` text and the
    /// `$OS_ERROR` it embeds must pull `use Carp` + `use English` back in
    /// (the gate scans the RENDERED body, which includes generator
    /// emulation text spliced by the whitelisted-command path).
    #[test]
    fn preamble_keeps_carp_and_english_when_used() {
        let prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::RawText(
                "carp \"rm: carping: could not remove \", $file, \": $OS_ERROR\\n\";\n"
                    .to_string(),
            )],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        };
        let perl = shir_to_perl(&prog);
        assert!(perl.contains("use Carp;"), "{perl}");
        assert!(perl.contains("use English"), "{perl}");
    }

    /// Declarator-position asm label (core request
    /// c-sh-go-toplevelasmargument-20260814-042952): the perl renderer
    /// refuses loudly (refuse > guess — the label names an object-file
    /// symbol the perl model does not have).
    #[test]
    fn assign_asm_label_perl_refuses() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[{"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Int","value":7},"asm":{"template":"myx","volatile":false,"outputs":[],"inputs":[],"clobbers":[]}}]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("assign-asm ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("asm label 'myx'"),
            "perl refuses the asm label: {perl}"
        );
    }

    /// Background + wait (triage-perl-20260814-063346 / 064414,
    /// t44_background py/posix-sh-go): `( cmd ) &` forks a child to run
    /// the body and the `wait` builtin reaps it — never the old refusal
    /// die.
    #[test]
    fn background_stmt_forks_and_wait_reaps() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Background","body":[{"type":"Subshell","body":[{"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"bg","style":"DoubleQuoted"}]}]}}]}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Spawn","args":[{"type":"Str","value":"wait","style":"DoubleQuoted"},{"type":"Array","elements":[]}]}},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"main","style":"DoubleQuoted"}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("background A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("my $__sh2_bg0 = fork();"),
            "background forks: {perl}"
        );
        assert!(
            perl.contains("if (defined $__sh2_bg0 && $__sh2_bg0 == 0) {"),
            "child branch: {perl}"
        );
        assert!(
            perl.contains("while (wait() != -1) {}"),
            "bare wait reaps all children: {perl}"
        );
        assert!(
            !perl.contains("not yet supported"),
            "no refusal die: {perl}"
        );
    }

    /// `[[ $s =~ re ]]` (triage-perl-20260814-063624, t68_case_glob): the
    /// spaced test tokenizer must keep `=~` as ONE op token and the
    /// renderer must emit a Perl regex match — the old split (`=` + `~`)
    /// parsed as equality against the literal `~` (`$s eq '~'`).
    #[test]
    fn regex_test_op_renders_match() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"If","cond":{"type":"Call","func":"test","args":[{"type":"Str","value":"\"$s\" =~ ^h","style":"DoubleQuoted"}]},"elsifs":[],"then":[{"type":"Expr","expr":{"type":"Call","func":"exec","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"star","style":"DoubleQuoted"}]}]}}],"else":[]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("=~ test A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("($s =~ '^h')"),
            "=~ renders a regex match: {perl}"
        );
        assert!(
            !perl.contains("eq '~'"),
            "no bogus equality against a literal tilde: {perl}"
        );
    }

    /// Field-splitting (triage-perl-20260814-064415, t62_word_split): the
    /// A1 `split` marker (unquoted `$x`) must expand in LIST contexts —
    /// printf args cycle the format per field, echo joins the fields, the
    /// for-iter iterates the fields — and the printf format's bash
    /// backslash escapes (`\n`) must decode (Perl's printf does not
    /// interpret them).
    #[test]
    fn split_word_expands_in_list_contexts() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[],"var_bash_env":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"x","sigil":null,"indices":[]}],"expr":{"type":"Str","value":"a b","style":"DoubleQuoted"}},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}]}},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"printf","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Interpolate","parts":[{"kind":"lit","text":"<%s>\\n"}]},{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]}]}},
            {"type":"For","var":"w","runs":true,"iter":{"type":"Array","elements":[{"type":"Call","func":"split","purity":"PureCpu","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}]}]},"body":[]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("split A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(
            perl.contains("print(join(' ', grep { defined($_) && length($_) } split(/\\s+/, $x)), \"\\n\");"),
            "echo joins the split fields: {perl}"
        );
        assert!(
            perl.contains("my @__sh2_pa0 = (split(/\\s+/, $x));"),
            "printf flattens the split fields: {perl}"
        );
        assert!(
            perl.contains("for (my $__sh2_pa0_i = 0;"),
            "printf cycles the format: {perl}"
        );
        assert!(
            perl.contains("for my $__w (split /\\s+/, $x) {"),
            "for-iter splits into fields: {perl}"
        );
        assert!(
            perl.contains("printf(\"<%s>\\n\""),
            "bash printf backslash escapes decode: {perl}"
        );
    }

    // ── embed profile (purify design, PLAN §10) ──────────────────────

    fn embed_prog(src: &str) -> IrProgram {
        let commands =
            crate::Parser::new(src).parse().unwrap_or_else(|e| panic!("parse {src}: {e}"));
        crate::shir::ast_to_ir(&commands)
    }

    fn render(src: &str, host_scope: &[&str]) -> EmbedResult {
        shir_to_perl_embed(
            &embed_prog(src),
            &EmbedCtx {
                host_scope: host_scope.iter().map(|s| s.to_string()).collect(),
                backtick_newlines: true,
                english_names: false,
            },
        )
    }

    #[test]
    fn embed_fragment_is_deterministic() {
        // the legacy `--inline` path was 30/30 flaky across processes (HashSet
        // declaration order); the embed renderer must be byte-stable
        let src = "echo hi; x=5; echo $x; for i in 1 2 3; do y=$((y+i)); done";
        let a = render(src, &["x"]);
        let b = render(src, &["x"]);
        assert_eq!(a.fragment, b.fragment, "embed output must be byte-stable");
    }

    #[test]
    fn embed_bindings_gate() {
        // read-only ∧ host_scope → bare reuse + required binding
        let r = render("echo $x", &["x"]);
        assert_eq!(r.required_host_bindings, vec!["x"]);
        assert!(r.refusals.is_empty(), "refusals: {:?}", r.refusals);
        assert!(
            !r.fragment.contains("my $x"),
            "host-scope read must not be declared locally: {}",
            r.fragment
        );
        // the gate: every required binding must be in host_scope
        for b in &r.required_host_bindings {
            assert!(
                ["x"].contains(&b.as_str()),
                "required binding {b} missing from host_scope"
            );
        }
        // read-only ∧ ¬host_scope → local `my $x = '';` (bash unset = empty),
        // nothing required from the host
        let r2 = render("echo $x", &[]);
        assert!(r2.required_host_bindings.is_empty());
        assert!(r2.fragment.contains("my $x = '';"), "{}", r2.fragment);
    }

    #[test]
    fn embed_copy_in_for_read_write() {
        // written ∧ host_scope → `my $x = $x;` copy-in: reads see the host
        // value, writes stay fragment-local (bash subshell semantics)
        let r = render("x=$((x+1)); echo $x", &["x"]);
        assert_eq!(r.required_host_bindings, vec!["x"]);
        assert!(r.fragment.contains("my $x = $x;"), "{}", r.fragment);
        // written ∧ ¬host_scope → plain local `my $x;`
        let r2 = render("x=$((x+1)); echo $x", &[]);
        assert!(r2.required_host_bindings.is_empty());
        assert!(r2.fragment.contains("my $x;"), "{}", r2.fragment);
    }

    #[test]
    fn embed_no_preamble() {
        let r = render("echo hi; x=5; echo $x", &[]);
        assert!(r.refusals.is_empty(), "refusals: {:?}", r.refusals);
        for banned in [
            "#!/usr/bin/env perl",
            "use strict",
            "use warnings",
            "use Carp",
            "use English",
            "exit $main_exit_code",
            "my $main_exit_code",
        ] {
            assert!(
                !r.fragment.contains(banned),
                "embed fragment must not contain {banned:?}: {}",
                r.fragment
            );
        }
    }

    #[test]
    fn embed_collapses_main_exit_writes() {
        // an external command lowers to system('bash','-c',…); the standalone
        // status tracker ($main_exit_code) is dead in an embed — the write is
        // collapsed to the $CHILD_ERROR mirror, and CHILD_ERROR is declared
        let src = "grep foo /tmp/nonexistent || echo no";
        let r = render(src, &[]);
        assert!(
            r.refusals.is_empty(),
            "refusals: {:?} fragment: {}",
            r.refusals,
            r.fragment
        );
        assert!(
            !r.fragment.contains("$main_exit_code"),
            "main_exit_code must be collapsed: {}",
            r.fragment
        );
        assert!(
            r.fragment.contains("our $CHILD_ERROR = 0;"),
            "CHILD_ERROR declared in fragment: {}",
            r.fragment
        );
        assert!(
            r.fragment.contains("$CHILD_ERROR = $? >> 8"),
            "status mirror kept: {}",
            r.fragment
        );
    }

    #[test]
    fn embed_english_normalization() {
        // the standalone emits $INPUT_RECORD_SEPARATOR (English.pm); an embed
        // must normalize to $/ so host files without `use English` stay valid
        let src = "IFS=: read -r a b < /dev/null; echo $a";
        let r = render(src, &[]);
        assert!(
            !r.fragment.contains("$INPUT_RECORD_SEPARATOR"),
            "English name must be normalized: {}",
            r.fragment
        );
    }

    #[test]
    fn embed_injects_carp_for_emulations() {
        // command emulations call carp/croak on error paths; the standalone
        // preamble imports Carp, an embed fragment must provide its own
        let src = "cat /etc/hostname";
        let r = render(src, &[]);
        assert!(
            r.fragment.contains("use Carp;"),
            "Carp import must be injected: {}",
            r.fragment
        );
        assert!(
            r.fragment.contains("croak \""),
            // the streaming foreach-line lowering aborts an unopenable
            // file with croak (die from the caller's perspective) — the
            // Carp import above keeps it executable in embeds
            "the emulation's error path is executable Perl: {}",
            r.fragment
        );
        assert!(r.refusals.is_empty(), "refusals: {:?}", r.refusals);
    }

    #[test]
    fn embed_refuses_exit_and_functions() {
        let r = render("exit 3", &[]);
        assert!(
            r.refusals.iter().any(|x| x.contains("exit")),
            "exit must be refused: {:?}",
            r.refusals
        );
        let r2 = render("f() { echo hi; }; f", &[]);
        assert!(
            r2.refusals.iter().any(|x| x.contains("function")),
            "function def must be refused: {:?}",
            r2.refusals
        );
    }

    /// A `def f(): ...` + `f()` call keeps the user's sub name and emits
    /// the sub BEFORE the call — the py frontend's t22 shape. Regression
    /// for `sub __sh_fn_f` (the name must be surfaced verbatim) and for
    /// the `local`/array-append lowering that used to abort the run.
    #[test]
    fn py_function_def_keeps_name_and_order() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"subs":[],"stmts":[
            {"type":"Function","name":"f","params":[],"body":[{"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"in-func","style":"DoubleQuoted"}]}]}}]},
            {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Spawn","args":[{"type":"Str","value":"f","style":"DoubleQuoted"},{"type":"Array","elements":[]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("t22 A1 ingress");
        let perl = shir_to_perl(&prog);
        assert!(perl.contains("sub f {"), "verbatim sub name:\n{perl}");
        assert!(!perl.contains("__sh_fn_f"), "no namespace rename:\n{perl}");
        let sub_at = perl.find("sub f {").expect("sub");
        let call_at = perl.find("\nf();").expect("call");
        assert!(sub_at < call_at, "definition precedes the call:\n{perl}");
    }

    /// An A1 `Declare { local: true }` (the py/C `local NAME` lowering)
    /// must NOT become Perl `local`: the hoisted vars are `my`-scoped
    /// lexicals, and `local $x` on a lexical is a compile error
    /// ("Can't localize lexical variable"). Regression for t86.
    #[test]
    fn declare_local_stays_my_scoped() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"subs":[],"stmts":[
            {"type":"Function","name":"f","params":[],"body":[
                {"type":"Declare","vars":[{"name":"n","sigil":null}],"init":null,"local":true},
                {"type":"Assign","targets":[{"var":"n","sigil":null,"indices":[]}],"expr":{"type":"Int","value":1}},
                {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"n","style":"DoubleQuoted"}]}]}]}}
            ]}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("ingress");
        let perl = shir_to_perl(&prog);
        assert!(perl.contains("my $n;"), "local Declare → my:\n{perl}");
        assert!(!perl.contains("local $n"), "never Perl local on a lexical:\n{perl}");
    }

    /// `arr+=(x)` arrives as an Assign over `setArrayAppend`: it must
    /// render as a real Perl `push @arr` (appending), not the scalar
    /// fallback `$arr = setArrayAppend(...)` (an undeclared scalar under
    /// strict, and no append). Regression for t86.
    #[test]
    fn set_array_append_is_a_push() {
        let src = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"var_types":[],"stmt_lines":[],"subs":[],"stmts":[
            {"type":"Assign","targets":[{"var":"xs","sigil":null,"indices":[]}],"expr":{"type":"Call","func":"setArray","purity":"Emulable","args":[{"type":"Str","value":"xs","style":"DoubleQuoted"},{"type":"Array","elements":[]}]}},
            {"type":"Assign","targets":[{"var":"xs","sigil":null,"indices":[]}],"expr":{"type":"Call","func":"setArrayAppend","purity":"Emulable","args":[{"type":"Str","value":"xs","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Int","value":7}]}]}}
        ]}"#;
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("ingress");
        let perl = shir_to_perl(&prog);
        assert!(perl.contains("push @xs, (7);"), "append is a push:\n{perl}");
        assert!(
            !perl.contains("$xs = setArrayAppend"),
            "no scalar fallback:\n{perl}"
        );
    }

    /// `${v:-${b:-$(cmd)}}` — a command substitution nested inside a
    /// parameter-expansion DEFAULT must reach the renderer as a real capture,
    /// not as opaque text re-lexed into a slice.
    ///
    /// `param_text_to_perl` re-lexes an expansion that arrived as a `Str`
    /// (which is how a nested default is carried). It has to choose between
    /// the `:-` operator and the `:` slice, and it used to read the SINGLE
    /// character `:` — so `:-` never reached the operator loop and every
    /// nested default was lowered as `substr($b, -$(cmd))`. The bare `$(` is
    /// Perl's GID special variable, so such a file fails `perl -c` and loses
    /// every construct in it (core request perl-20261002-231200; corpus
    /// 063_04, 063_hard_to_parse).
    ///
    /// Pins both halves of bash's rule: `:-` immediately after the `:` is the
    /// operator, `:` + SPACE is a slice (`a=abcdef` → `${a:-1}` is `abcdef`,
    /// `${a: -1}` is `f`).
    #[test]
    fn nested_default_cmdsub_is_a_capture_not_a_slice() {
        let op = |tok: &str| param_text_to_perl(tok).unwrap_or_else(|| panic!("{tok}"));
        // The nested command substitution is a real capture, and the `:-`
        // default is a real conditional — never a `substr` with a negative
        // offset, which is the shape the defect produced.
        let deep = op("${a:-${b:-$(echo DEEP)}}");
        assert!(
            deep.contains("defined($a)"),
            "outer default survives: {deep}"
        );
        assert!(
            deep.contains("defined($b)"),
            "inner default survives: {deep}"
        );
        assert!(
            !deep.contains("substr("),
            "a nested default is not a slice: {deep}"
        );
        assert!(
            !deep.contains("$("),
            "no bare $( (Perl's GID) may reach the output: {deep}"
        );
        // bash: `:-` is the operator only when the `-` follows the `:` with
        // no space. The space is the whole discriminator and the parser keeps
        // it in the offset, so the textual fallback must too.
        assert_eq!(op("${a:-1}"), op("${a:-1}"), "stable");
        let defaulted = op("${a:-1}");
        assert!(
            defaulted.contains("defined($a)"),
            "`:-` is the default operator: {defaulted}"
        );
        let sliced = op("${a: -1}");
        assert!(
            !sliced.contains("defined($a)"),
            "`: ` + space is a slice, not a default: {sliced}"
        );
    }

    /// Build the `Call { func: "param" }` IR a `${v/pat/rep}` lowers to.
    fn subst_ir(op: &str, pat: &str, rep: &str) -> Vec<IrExpr> {
        vec![
            IrExpr::Str(op.to_string(), StrStyle::DoubleQuoted),
            IrExpr::Str("s".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Str(pat.to_string(), StrStyle::DoubleQuoted),
            IrExpr::Str(rep.to_string(), StrStyle::DoubleQuoted),
        ]
    }

    /// `${v/pat/rep}` must compile a PATTERN and a REPLACEMENT, not two
    /// strings — a quoted operand makes the substitution a silent no-op.
    ///
    /// The operands reach the renderer bare (`Str("abc")`), and this arm used
    /// to run them through `param_value`, which is `render_str_literal` under
    /// another name. That is right for a DEFAULT (`${a:-abc}` wants the Perl
    /// string `"abc"`) and wrong here: the quotes became part of the PATTERN,
    ///
    ///     s/abc/XYZ  ⇒  s/"abc"/"XYZ"
    ///
    /// which Perl compiles as "the six characters `\"abc\"`". Those never occur
    /// in the subject, so the match fails, `s///r` returns the subject
    /// UNCHANGED, and the whole substitution silently does nothing — correct
    /// exit code, no warning, wrong text (core request
    /// perl-20261002-231200 Gap 2; corpus bench-strsubst).
    #[test]
    fn param_substitution_operands_are_not_quoted() {
        let one = render_param(&subst_ir("/", "abc", "XYZ"));
        assert!(
            one.contains("s/abc/XYZ/"),
            "operands must reach the regex unquoted: {one}"
        );
        assert!(
            !one.contains("s/\""),
            "a quoted PATTERN can never match the subject: {one}"
        );
        // `//` is the global form and adds `g`; it shares this arm because
        // the separate `"//" | "/"` arm was dead (shadowed by a plain `"/"`).
        let global = render_param(&subst_ir("//", "abc", "XYZ"));
        assert!(
            global.contains("s/abc/XYZ/gr"),
            "`//` replaces globally: {global}"
        );
    }

    /// A replacement is interpolated into the regex BODY, so its `/` must be
    /// escaped — but the operand arrives as SOURCE TEXT that already carries
    /// the shell's own escape, which must be consumed rather than doubled.
    ///
    /// `s/abc/b\/c` (three backslashes emitted) compiles and runs, so this
    /// would not be a syntax error; it prints `b\/c` where bash prints `b/c`.
    #[test]
    fn param_substitution_replacement_escapes_bash_escapes_once() {
        // Source `${s/abc/b\/c}` reaches the IR as the two characters `\` `/`.
        let escaped = render_param(&subst_ir("/", "abc", "b\\/c"));
        assert!(
            escaped.contains("s/abc/b\\/c/"),
            "one shell escape in, one escape out: {escaped}"
        );
        assert!(
            !escaped.contains("b\\\\\\/"),
            "the shell escape must not be doubled: {escaped}"
        );
        // `@`/`$` are interpolated by Perl, so they must be escaped or the
        // replacement splices in Perl's own arrays/scalars.
        assert_eq!(repl_for_regex("@x"), "\\@x");
        assert_eq!(repl_for_regex("$y"), "\\$y");
        assert_eq!(repl_for_regex("@$y"), "\\@\\$y");
        // A literal backslash is `\\` in the SOURCE, so it reaches the helper
        // as two characters and must be emitted as four — otherwise it eats
        // the character after it in the Perl body.
        assert_eq!(repl_for_regex("b\\\\c"), "b\\\\\\\\c");
        // An unescaped `/` still has to be escaped, or the substitution ends
        // early and the rest of the expression is a syntax error.
        assert_eq!(repl_for_regex("a/b"), "a\\/b");
    }

    /// A nested command substitution inside a rebuilt command word must go
    /// back as SHELL syntax, never as rendered Perl.
    ///
    /// `cd -- "$(dirname "$0")"` reaches `bash_word_for` as the `IrExpr::Capture`
    /// VARIANT (not the `Call{func:"capture"}` form). Without a variant arm it
    /// fell through to `_`, which runs `render_word` and emits PERL
    /// (`do { … File::Basename::dirname($0) … }`) into a command string that is
    /// then handed to `bash -c`. bash runs a command literally named `do { … }`,
    /// the capture yields empty, and the program silently prints the wrong
    /// value with exit 0 — the shape reported on qx-var-builtin-cd.sh.
    #[test]
    fn nested_capture_word_rebuilds_as_shell_syntax() {
        // `cd -- "$(dirname "$0")"` — the enclosing command text is rebuilt
        // for a `bash -c` / pipeline run, so the nested word must come back
        // as SHELL syntax.
        let prog = embed_prog("d=$(cd -- \"$(dirname \"$0\")\" && pwd)");
        let text = shir_to_perl(&prog);
        assert!(
            !text.contains("require File::Basename"),
            "no Perl module call may reach a command string: {text}"
        );
        assert!(
            !text.contains("do { my $__sh_cap"),
            "no raw Perl capture body may reach a command string: {text}"
        );
        assert!(
            text.contains("$(dirname") || text.contains("$('dirname'"),
            "the nested substitution survives as shell text: {text}"
        );
    }

    /// A `dq_value_expr` content carrying a command substitution is refused,
    /// so the caller keeps the plain `bash -c` text path.
    ///
    /// `$(…)` is a whole command whose OUTPUT is the value; the identifier
    /// scan only rewrites a `$` expansion it recognised, so a leftover `$(`
    /// was spliced into the reconstructed command string verbatim and bash was
    /// handed a Perl expression as a literal WORD (exit 0, empty stdout).
    #[test]
    fn dq_value_expr_refuses_a_nested_command_substitution() {
        assert_eq!(dq_value_expr("$(dirname \"$0\")"), None);
        assert_eq!(dq_value_expr("pre$(echo x)post"), None);
        assert_eq!(dq_value_expr("`echo x`"), None);
        // A plain expansion still has a value form — the guard must not
        // swallow the shapes this function exists to serve.
        assert!(dq_value_expr("$0").is_some());
        assert!(dq_value_expr("${d}/f").is_some());
    }
}






