//! EchoFold — lower a constant `echo` to a native `IrStmt::Output`.
//!
//! A shell `echo` arrives from the frontends as
//! `IrStmt::Expr(Call { func: "exec"|"builtin", args: [Str("echo"), Array(words)] })`.
//! Every backend then has to re-implement the echo semantics (`-n`/`-e`/
//! `-E` flags, word joining, the trailing newline) — and the ones that
//! do it through a runtime helper (`__sh_print_words`, `sh2.printWords`)
//! pay for it at run time even when the whole command is a literal.
//!
//! When all words are compile-time constants (a bare literal, or a
//! quoted string whose only part is a literal — no expansion, no glob,
//! no arith guard) the entire output is known at transform time. The
//! fold rewrites it to
//!
//! ```text
//! IrStmt::Output { value: IrExpr::Str(text, DoubleQuoted), newline, target: None }
//! ```
//!
//! whose text already includes the space-joined words, the `-e`
//! unescaping and (when `newline`) the trailing `\n`. `IrStmt::Output`
//! is the node every backend renders natively (`print`/`say`, `print!`,
//! `System.out.print`, `fmt.Fprint`, …), so the helper is skipped in
//! every backend at once instead of each renderer re-deriving the
//! peephole.
//!
//! # Why this is sound
//!
//! * Only literal words fold; any `$var`/`$(...)`/glob word leaves the
//!   statement untouched (the runtime helper keeps its job).
//! * `echo`'s flags are only recognised in leading position and only
//!   from literal words, matching the shell's own rule.
//! * `-e` unescaping reuses [`echo_esc`], the same state machine the
//!   `--shir` contract documents for echo; the fold and any runtime
//!   unescaper must agree (they are kept in lockstep).
//! * `echo` always succeeds (status 0). The folded `Output` carries that
//!   status implicitly (it cannot fail); renderers that track `$?` treat
//!   `Output` as status 0, so dropping the `exec` call cannot change the
//!   script's exit status.

use crate::ir::{InterpPart, IrExpr, IrProgram, IrStmt, StrStyle};

/// Fold constant `echo`s into native `Output` statements — see the
/// module docs.
pub struct EchoFold;

impl super::Transform for EchoFold {
    fn name(&self) -> &'static str {
        "echo_fold"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &super::PassContext) {
        // Pass 1 — the LEGACY reach, both folds: exactly the containers
        // this pass has always visited. Measured against the render-side
        // matchers (see `fold_echo_narrow`'s docs), `echo` folding is only
        // proven safe inside them.
        fold_narrow(&mut prog.stmts);
        for sub in &mut prog.subs {
            fold_narrow(&mut sub.body);
        }
        // Pass 2 — the WIDE reach, `printf` only: every container and
        // expression position, so a constant printf behind an Arrow /
        // pipeline stage / subshell folds too.
        fold_wide(&mut prog.stmts, false);
        for sub in &mut prog.subs {
            fold_wide(&mut sub.body, false);
        }
    }
}

/// The pass's ORIGINAL reach, folding both `echo` and `printf`.
///
/// Kept verbatim as a separate walk because several render-side native
/// lowerings pattern-match the shape `Expr(Call { "echo", words })` —
/// the `&&`/`||` chain lowering, the subshell-capture arm, the bc
/// producer arm and the uu-ffi preamble's shell-out census all key on
/// it. Moving an `echo` OUT of these containers into `Output` made those
/// matchers refuse and fall back to shell-outs, and two of those
/// fallbacks are lossy: 4 measured equivalence regressions
/// (`019_grep_regex`, `bc-native-capture`, `063_13_nested_subshells`,
/// `parse-multi-command-while-condition`) — all four `echo`, zero
/// `printf`.
///
/// So: `echo` stays on this narrow walk until those matchers accept the
/// canonical `Output` node, while [`fold_wide`] gives `printf` the full
/// traversal (`printf` has no shape-keyed matcher beyond
/// `pipeline_native`'s producer channels, which [`output_content`]
/// teaches about `Output`).
fn fold_narrow(stmts: &mut [IrStmt]) {
    for st in stmts.iter_mut() {
        match st {
            IrStmt::If {
                then,
                elsifs,
                else_,
                ..
            } => {
                fold_narrow(then);
                for (_, b) in elsifs.iter_mut() {
                    fold_narrow(b);
                }
                fold_narrow(else_);
            }
            IrStmt::For { body, .. } | IrStmt::While { body, .. } => fold_narrow(body),
            IrStmt::DoWhile { body, .. } => fold_narrow(body),
            IrStmt::Block(b) => fold_narrow(b),
            IrStmt::Redirect { inner, .. } => fold_narrow(inner),
            // A function BODY is the same statement list a `Block` is:
            // a standalone `echo` written inside `f() { … }` folds
            // exactly as it does at top level, so `f() { echo x > file; }`
            // reaches `WriteFile` and renders `fs::write` in Rust with no
            // sink machinery (`function_bodies_are_runtime_free_when_
            // redirect_free`), and every other backend drops the same
            // word-printing polyfill. The four matcher-keyed containers
            // fold_narrow's doc names are NOT this one — the chain /
            // subshell-capture / bc-producer arms key on shapes this arm
            // does not create (it only ever replaces a standalone
            // `Expr(echo)` statement), so their `Expr(Call echo)` matchers
            // keep the shapes they were measured against.
            IrStmt::Function {
                body, named_blocks, ..
            } => {
                fold_narrow(body);
                for (_, b) in named_blocks.iter_mut() {
                    fold_narrow(b);
                }
            }
            _ => {}
        }
        if let Some(out) = constant_echo(st)
            .or_else(|| dynamic_echo(st))
            .or_else(|| constant_printf(st))
        {
            *st = out;
        }
    }
}

/// Recurse through EVERY statement container and expression position.
///
/// `fold_echo` selects the policy: `false` (the wide pass) folds only a
/// constant `printf`, leaving an `echo` in place wherever it sits — see
/// [`fold_narrow`] for why `echo` is restricted to the legacy containers.
///
/// The traversal itself is deliberately total — the same shape
/// `strip_cfor` uses — because a constant `printf` this pass never VISITS
/// is the word-printing polyfill every backend then pays for (the whole
/// `__sh_printf` cluster in rust, plus the runtime print helper). The
/// statement-only walk this replaced missed the two places the corpus
/// actually puts most of them:
///
///   * **`IrExpr::Arrow` / `Lambda` bodies** — a pipeline stage arrives as
///     `Call { "pipeline", [Arrow, …] }`, and loop-cond/loop-body arrows,
///     capture bodies and a `for ((…))` inside `( … ) | sort` are all
///     expression-carried too. A walk that only follows `Vec<IrStmt>`
///     fields never reaches them.
///   * **`Pipeline` stages, `Subshell`, `Background`, `Case` clauses,
///     `Try` arms, nested `Function` bodies, redirect targets** — statement
///     lists/expression positions the old match had no arm for.
///
/// Recursion runs BEFORE the fold so a nested constant that already
/// folded to `Output` is left alone (it is native), and so the fold sees
/// the post-fold shape of its children.
fn fold_wide(stmts: &mut [IrStmt], fold_echo: bool) {
    for st in stmts.iter_mut() {
        fold_wide_stmt(st, fold_echo);
    }
}

fn fold_wide_stmt(st: &mut IrStmt, fold_echo: bool) {
    match st {
        IrStmt::Expr(e) => fold_wide_expr(e, fold_echo),
        IrStmt::Output { value, .. } => fold_wide_expr(value, fold_echo),
        IrStmt::WriteFile { path, content, .. } => {
            fold_wide_expr(path, fold_echo);
            fold_wide_expr(content, fold_echo);
        }
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    fold_wide_expr(i, fold_echo);
                }
            }
            fold_wide_expr(expr, fold_echo);
        }
        IrStmt::Declare { init: Some(e), .. } => fold_wide_expr(e, fold_echo),
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements.iter_mut() {
                fold_wide_expr(e, fold_echo);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            fold_wide_expr(cond, fold_echo);
            fold_wide(then, fold_echo);
            for (c, b) in elsifs.iter_mut() {
                fold_wide_expr(c, fold_echo);
                fold_wide(b, fold_echo);
            }
            fold_wide(else_, fold_echo);
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            fold_wide(body, fold_echo);
            for e in excepts.iter_mut() {
                fold_wide(&mut e.body, fold_echo);
            }
            fold_wide(else_body, fold_echo);
            fold_wide(finally_body, fold_echo);
        }
        IrStmt::For { iter, body, .. } => {
            fold_wide_expr(iter, fold_echo);
            fold_wide(body, fold_echo);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            fold_wide(init, fold_echo);
            fold_wide_expr(cond, fold_echo);
            fold_wide(step, fold_echo);
            fold_wide(body, fold_echo);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            fold_wide_expr(cond, fold_echo);
            fold_wide(body, fold_echo);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => fold_wide(b, fold_echo),
        IrStmt::Redirect { inner, redirects } => {
            fold_wide(inner, fold_echo);
            // the redirect TARGET is an expression too — `> $(printf …)`
            // and a heredoc body can carry an arrow the fold belongs in.
            for r in redirects.iter_mut() {
                fold_wide_expr(&mut r.target, fold_echo);
            }
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            fold_wide_expr(discriminant, fold_echo);
            for c in clauses.iter_mut() {
                fold_wide(&mut c.body, fold_echo);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                fold_wide(stage, fold_echo);
            }
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            fold_wide(body, fold_echo);
            for (_, b) in named_blocks.iter_mut() {
                fold_wide(b, fold_echo);
            }
        }
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            fold_wide_expr(cmd, fold_echo);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                fold_wide_expr(a, fold_echo);
            }
            for (_, v) in env.iter_mut() {
                fold_wide_expr(v, fold_echo);
            }
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            fold_wide_expr(expr, fold_echo)
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => fold_wide_expr(e, fold_echo),
        IrStmt::Ext(node) => {
            for c in node.children_mut() {
                fold_wide_stmt(c, fold_echo);
            }
        }
        // no statement list can hide in these: Continue, Break, Label,
        // Goto, Require, RawText, Select, Asm, Shaderize, and the
        // armless Return(None)/Exit(None).
        _ => {}
    }
    let out = if fold_echo {
        constant_echo(st).or_else(|| constant_printf(st))
    } else {
        constant_printf(st)
    };
    if let Some(out) = out {
        *st = out;
    }
    // Children first, so the inner echo/printf is already a native
    // `Output` by the time the enclosing redirect is inspected.
    if matches!(st, IrStmt::Redirect { .. }) {
        redirected_output_to_writefile(st);
    }
}

/// `Redirect { inner: [Output], fd1 w|a }` → native [`IrStmt::WriteFile`].
///
/// The output half of this shape is already native — [`constant_echo`] /
/// [`constant_printf`] folded `echo "x" > f`'s producer into an
/// `IrStmt::Output` — but the surrounding redirect still forces every
/// backend through its generic redirect machinery: Rust installs the
/// file in `__SH_OUTFILE_TL`, runs the 22-line `__sh_print_words` helper
/// and restores it (5 statements for a literal write), Perl builds the
/// `select()`-based fd swap, the shell backends rebuild shell text.
///
/// Every backend already renders `IrStmt::WriteFile` natively and
/// simply (`fs::write` / `open-print-close` / `sh2.writeFile` /
/// `printf '%s' … > f`), and it is the node the ESTree renderer's own
/// `echo > file` lift targets — so lowering to it removes the same
/// boilerplate everywhere at once instead of each renderer re-deriving
/// the peephole. This is the shared twin of that backend-local lift.
///
/// # Why this is sound (refuse > guess)
///
/// * Exactly ONE redirect, fd 1 (a `2>` writes stderr the producer never
///   emits; a multi-spec redirect would lose siblings), mode `w`/`wc`
///   (truncate) or `a` (append → `WriteFile.append`).
/// * Exactly ONE inner statement, and it is an `Output` with
///   `target: None` (a filehandle target is not this file).
/// * An `&N` fd-dup or `-` target lives in the fd table, not the
///   filesystem — `WriteFile` would create a file literally named `&1` —
///   and an `Arrow` target is a process substitution (`>(cmd)`), whose
///   bytes belong to that command's stdin. Both stay on the redirect
///   machinery; only path-shaped targets (Str / Var / Ident /
///   Interpolate) fold.
/// * Byte/glob markers (`\u{1}…`) are refused: the renderers decode them
///   on the PRINT path (Rust's `__sh_out_bytes`, the runtime's `emit`),
///   and a raw `fs::write` would put the marker text in the file.
/// * Content is built exactly as `Output` would print it: the literal
///   plus `\n` when `newline`, or `Interp[expr, "\\n"]` for a dynamic
///   value — the same node `"$x"` lowers to, so every renderer's
///   interpolation applies unchanged.
/// * Status: a successful write is 0 in bash, and `WriteFile` is a
///   status-0 statement for Rust/Java/estree already (their arms store
///   it); the Perl arm records it too.
/// The null command with NO arguments — the `:` of `: > f`. It produces
/// nothing, so inside a redirect it is an empty file write; `: word` is
/// a command with arguments and stays put. Like [`is_var_read`] this is
/// a shape test, not an evaluation: nothing here is ever executed.
fn is_noop_colon(e: &IrExpr) -> bool {
    let IrExpr::Call { func, args, .. } = e else {
        return false;
    };
    (func == "builtin" || func == "exec")
        && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == ":")
        && (matches!(args.get(1), None)
            || matches!(args.get(1), Some(IrExpr::Array(a)) if a.is_empty()))
}

pub fn redirected_output_to_writefile(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else {
        return false;
    };
    let [r] = redirects.as_slice() else {
        return false;
    };
    if r.fd.unwrap_or(1) != 1 {
        return false;
    }
    let append = match r.mode.as_str() {
        "w" | "wc" => false,
        "a" => true,
        _ => return false,
    };
    // `: > f` has no `Output` half to wait for — it produces nothing — so
    // give it the empty one and let the rest of this fold (target rules,
    // content building, status) apply unchanged: `w` truncates, `a`
    // touches, both exactly what bash does. Only here: a bare `:` with no
    // redirect never reaches this function, so that statement keeps its
    // shape.
    if let [IrStmt::Expr(e)] = inner.as_slice() {
        if is_noop_colon(e) {
            inner[0] = IrStmt::Output {
                value: IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                newline: false,
                target: None,
            };
        }
    }
    let [IrStmt::Output {
        value,
        newline,
        target: None,
    }] = inner.as_slice()
    else {
        return false;
    };
    // Path-shaped targets only. A literal `&1`/`-` is an fd-table target
    // (a file literally named `&1` is not what bash writes), and an
    // `Arrow` target is a process substitution (`>(cmd)`) whose bytes
    // belong to that command's stdin — neither is a filesystem path, so
    // refusing keeps the statement on the redirect machinery that models
    // them. Str / Var / Ident / Interpolate are the path spellings every
    // renderer already lowers as a path expression.
    match &r.target {
        IrExpr::Str(s, _) if s.starts_with('&') || s == "-" => return false,
        IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_) | IrExpr::Interpolate(_) => {}
        _ => return false,
    }
    // A CALL anywhere in the value refuses the fold — four independent
    // reasons, all measured on `h() { echo in-h; }; h > file`:
    //
    //  * the WORD path and the WRITE path render calls differently
    //    (`words_expr` wraps `fnValue` as `vec![{ h() … }]`, the content
    //    path renders the bare call), so the bytes could diverge;
    //  * `-> String` elision (`if value_used`) walks `Output` values but
    //    not `WriteFile` content, so the callee's return type was elided
    //    while the call site still formatted its value — rustc E0277;
    //  * a guarded `arith` word stores `__SH_ARITH_WORD_FAIL` and the
    //    word printer SUPPRESSES the whole command — while `fs::write`
    //    would write regardless;
    //  * bash does not open the target when a failing expansion
    //    suppresses the command, so the write itself would be new
    //    behavior.
    //
    // Literal and interpolated text (the 28-file corpus win) still
    // folds. Refuse > guess.
    fn contains_call(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { .. } if is_var_read(e) => false,
            IrExpr::Call { .. } | IrExpr::Arrow(_) | IrExpr::Lambda { .. } => true,
            IrExpr::Interpolate(parts) => parts
                .iter()
                .any(|p| matches!(p, InterpPart::Expr(x) if contains_call(x))),
            IrExpr::BinOp { lhs, rhs, .. } => contains_call(lhs) || contains_call(rhs),
            IrExpr::MethodCall { obj, args, .. } => {
                contains_call(obj) || args.iter().any(contains_call)
            }
            IrExpr::Array(items) => items.iter().any(contains_call),
            _ => false,
        }
    }
    if contains_call(value) {
        return false;
    }
    let content = match value {
        IrExpr::Str(s, _) => {
            if s.contains('\u{1}') {
                return false;
            }
            let mut t = s.clone();
            if *newline {
                t.push('\n');
            }
            IrExpr::Str(t, StrStyle::DoubleQuoted)
        }
        v if *newline => IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(v.clone())),
            InterpPart::Lit("\n".to_string()),
        ]),
        v => v.clone(),
    };
    *st = IrStmt::WriteFile {
        path: r.target.clone(),
        content,
        append,
    };
    true
}

/// Expression-level recursion: statements live INSIDE expressions in this
/// IR (a pipeline stage is an `Arrow` argument; a capture body can be an
/// arrow), so reaching them is what makes the fold total. An `Arith` AST
/// never carries statements, so it needs no recursion.
fn fold_wide_expr(e: &mut IrExpr, fold_echo: bool) {
    match e {
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => fold_wide(stmts, fold_echo),
        IrExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                fold_wide_expr(a, fold_echo);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            fold_wide_expr(obj, fold_echo);
            for a in args.iter_mut() {
                fold_wide_expr(a, fold_echo);
            }
        }
        IrExpr::Array(items) => {
            for a in items.iter_mut() {
                fold_wide_expr(a, fold_echo);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props.iter_mut() {
                fold_wide_expr(v, fold_echo);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            fold_wide_expr(lhs, fold_echo);
            fold_wide_expr(rhs, fold_echo);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            fold_wide_expr(cond, fold_echo);
            fold_wide_expr(then, fold_echo);
            fold_wide_expr(else_, fold_echo);
        }
        IrExpr::DefinedOr { expr, default } => {
            fold_wide_expr(expr, fold_echo);
            fold_wide_expr(default, fold_echo);
        }
        IrExpr::Index { key, .. } => fold_wide_expr(key, fold_echo),
        IrExpr::Capture { expr, .. } => fold_wide_expr(expr, fold_echo),
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    fold_wide_expr(x, fold_echo);
                }
            }
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            fold_wide_expr(iter, fold_echo);
            fold_wide_expr(elem, fold_echo);
            if let Some(c) = cond.as_mut() {
                fold_wide_expr(c, fold_echo);
            }
        }
        _ => {}
    }
}

/// `Some(Output)` when `st` is a constant `printf`: the format and every
/// argument are compile-time literals and [`printf_const_eval`] can
/// reproduce the runtime engine's bytes exactly.
///
/// The Rust renderer wraps every printf in `__sh_print_words(&[vec!
/// [__sh_printf(…)]]…)` plus the `__sh_printf`/`__sh_q_printf`/`__sh_atoi`
/// polyfill cluster even when the whole output is a compile-time string.
/// This fold lowers that shape to the same native `IrStmt::Output` the
/// constant-`echo` fold produces, so every backend prints it directly and
/// the helper prune drops the whole cluster wherever no other printf
/// survives (measured 2026-10-25: 27 all-const call sites still kept the
/// engine alive in 16 files — they now drop it wholesale).
///
/// Soundness — every refusal keeps the statement on the runtime path:
///   * any non-literal word (expansion, capture, arithmetic);
///   * no glob sentinel: `SH2GLOB` is the core's EXPAND-me
///     instruction, never literal text (the 064 lesson);
///   * `-v` is a target write / usage error the renderer prints verbatim —
///     leave that path byte-identical;
///   * a leading-dash format with arguments: bash parses printf options
///     (`-v`, `--`) before the format, so the word layout is ambiguous —
///     the runtime path parses it;
///   * anything [`printf_const_eval`] cannot reproduce from the runtime
///     `__sh_printf` engine (float conversions, unknown conversions).
///
/// `newline: false` — printf appends no newline of its own (the `\n`
/// lives inside the format, materialized by the eval), and Output's
/// status-0 contract matches printf's success status — exactly what the
/// runtime path reports.
fn constant_printf(st: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "printf" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    let (fmt_word, arg_words) = words.split_first()?;
    if let IrExpr::Str(f, _) = fmt_word {
        if f == "-v" {
            return None;
        }
    }
    let text = const_word_text(fmt_word)?;
    if text.contains('\u{1}') {
        return None;
    }
    // With arguments present a leading-dash word is printf's OWN option
    // syntax (`-v var …`, `--`), which shifts the format position — the
    // runtime path is the one that parses it.
    if !arg_words.is_empty() && text.starts_with('-') {
        return None;
    }
    let mut argv = Vec::with_capacity(arg_words.len());
    for a in arg_words {
        let Some(t) = const_word_text(a) else {
            // a DYNAMIC argument: the all-constant eval cannot run, but a
            // plain-`%s` format still lowers natively — see `printf_s_fold`
            return printf_s_fold(&text, arg_words);
        };
        if t.contains('\u{1}') {
            return None;
        }
        argv.push(t);
    }
    let out = printf_const_eval(&text, &argv)?;
    Some(IrStmt::Output {
        value: IrExpr::Str(out, StrStyle::DoubleQuoted),
        newline: false,
        target: None,
    })
}

/// `Some(Output)` when a printf's FORMAT is a compile-time literal whose
/// conversions are ALL plain `%s` — exactly one argument per specifier —
/// while the ARGUMENTS themselves may be dynamic.
///
/// This is [`constant_printf`]'s dynamic-argument twin: that fold needs
/// every word literal, this one needs only the format. The result is the
/// same native `IrStmt::Output`, built as `InterpPart::Lit` runs around
/// each argument expression, so every backend prints a plain string
/// instead of routing through the `__sh_printf`/`__sh_q_printf`/
/// `__sh_atoi`/`__sh_atou`/`__sh_atof` engine cluster. That cluster is the
/// largest surviving polyfill (58 of 563 emitted Rust files, ~135 lines
/// each), and `%s=[%s]\n`-shaped formats are the corpus's dominant printf
/// (21 of 126 call sites, 31 of 58 engine files use nothing but `%s`).
///
/// Soundness — every refusal keeps the statement on the runtime path:
///   * a format escape that can TRUNCATE (`\c`) or MINT a `%` (`\xHH`,
///     `\0nnn`): [`printf_s_split`] reads the RAW format, so such an escape
///     could hide a conversion from it while the engine still sees one —
///     or drop the tail of the output the split kept;
///   * anything that is not a PLAIN `%s`: flags/width/precision
///     (`%-10s`, `%.3s`) change the bytes, and every other conversion
///     (`%d`, `%q`, `%b`, floats, unknown) needs the engine's helpers;
///   * a specifier count that differs from the argument count: bash then
///     reuses the format or pads with empty strings, and an extra argument
///     word could carry an expansion this fold would never evaluate;
///   * the same refusals `constant_printf` applies first (`-v`, the
///     leading-dash option word, the `\u{1}` glob sentinel in the format).
///
/// `newline: false` — printf appends no newline of its own, exactly as in
/// the all-constant path.
fn printf_s_fold(text: &str, arg_words: &[IrExpr]) -> Option<IrStmt> {
    if arg_words.is_empty() {
        // no argument: the all-constant path owns this shape (and a
        // conversion-free format with a side-effecting argument must not
        // drop that argument here)
        return None;
    }
    if printf_escape_can_mint_conversion(text) {
        return None;
    }
    // an argument that expands to SEVERAL words makes bash count runtime
    // words against the specifiers (format reuse / empty padding) — a
    // count the static specifier check below cannot see
    if arg_words.iter().any(arg_expands_to_many_words) {
        return None;
    }
    let chunks = printf_s_split(text)?;
    let specs = chunks.len() - 1;
    if specs == 0 || specs != arg_words.len() {
        return None;
    }
    let mut parts: Vec<InterpPart> = Vec::with_capacity(chunks.len() + arg_words.len());
    for (i, chunk) in chunks.iter().enumerate() {
        // each chunk is conversion-free, so the engine port evaluates it
        // verbatim with no arguments — the SAME escape table, no re-port
        let lit = printf_const_eval(chunk, &[])?;
        if !lit.is_empty() {
            parts.push(InterpPart::Lit(lit));
        }
        if let Some(a) = arg_words.get(i) {
            match const_word_text(a) {
                Some(t) => {
                    if t.contains('\u{1}') {
                        return None;
                    }
                    // `%s` prints the word's bytes verbatim — same as the
                    // all-constant path would have baked in
                    parts.push(InterpPart::Lit(t));
                }
                None => {
                    if !push_interp_part(&mut parts, a) {
                        return None;
                    }
                }
            }
        }
    }
    let value = parts_value(parts);
    Some(IrStmt::Output {
        value,
        newline: false,
        target: None,
    })
}

/// Can this printf argument word expand to MORE than one word at runtime?
///
/// `printf '%s ' "${lines[@]}"` is ONE IR word but N printf arguments —
/// bash counts the expanded words against the format's specifiers and
/// reuses the format once per element — so the static specifier/argument
/// count in [`printf_s_fold`] would be lying (041_process_substitution_
/// mapfile: the fold printed the array's join where bash printed one round
/// per element). Those shapes stay on the runtime path, which passes the
/// expanded list straight through.
///
/// A `Capture` (`$(…)`) is deliberately NOT listed: quoted it is one word,
/// and the runtime engine already treats it as one argument — this fold
/// must not diverge from the runtime it replaces.
fn arg_expands_to_many_words(e: &IrExpr) -> bool {
    match e {
        IrExpr::Array(_) => true,
        IrExpr::Interpolate(ps) => ps
            .iter()
            .any(|p| matches!(p, InterpPart::Expr(x) if arg_expands_to_many_words(x))),
        IrExpr::Call { func, args } => match func.as_str() {
            // array/word-producing builtins: `${a[@]}`-shaped results
            "arrayItems" | "split" | "captureWords" | "words" => true,
            // `param(…)`: the sigil of `${a[@]}` / `${a[*]}` / `$@` / `$*`
            // (slice/params forms all carry it as a Str word)
            "param" => args
                .iter()
                .any(|a| matches!(a, IrExpr::Str(s, _) if s == "@" || s == "*")),
            _ => false,
        },
        _ => false,
    }
}

/// A plain variable read in its shIR spelling — `Call { "getVar",
/// [Str(name)] }`, the node `$name` lowers to — is opaque text exactly
/// like `IrExpr::Var`, not a call: it runs no callee, arms no
/// arithmetic guard, splits nothing and homes no parameter, so every
/// refusal reason below (all about a call's side effects) is untouched
/// by it. Only an IDENTIFIER name counts: `getVar` also spells the
/// special parameters (`$-`, `$?`, `$@`, `$*`, positionals …), each of
/// which its own renderer expands differently from a plain read — and
/// the word path and the write path would then disagree. Any other
/// `func`, a non-identifier name or extra arguments stay a call and
/// keep refusing.
fn is_var_read(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args, .. }
        if func == "getVar"
            && matches!(args.as_slice(), [IrExpr::Str(n, _)]
                if n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')))
}

/// Does this word contain a call (or an arithmetic node)?
///
/// Everything the echo CALL path does beyond producing text hangs off a
/// call: the backend's own `$((…))` evaluator and its failure guard, the
/// glob/split helpers, native joins such as `sortedIntJoin`, and — the
/// one that bit t89/t90 — the parameter probe that runs while those word
/// renders happen. Wrappers are walked so a call buried in an
/// interpolation, a condition or a concatenation is caught too; a plain
/// literal or variable reference is opaque text and safe to fold —
/// including a bare [`is_var_read`], which is how `$name` reaches here.
fn word_contains_call(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { .. } if is_var_read(e) => false,
        IrExpr::Call { .. } | IrExpr::Arrow(_) | IrExpr::Lambda { .. } => true,
        // `((…))` in word position goes to the same evaluator as
        // `Call { func: "arith" }`
        IrExpr::Arith(_) => true,
        // `$(…)` is declined separately, but a capture nested in a word is
        // the same refusal for the same reason
        IrExpr::Capture { .. } => true,
        IrExpr::Interpolate(ps) => ps
            .iter()
            .any(|p| matches!(p, InterpPart::Expr(x) if word_contains_call(x))),
        IrExpr::BinOp { lhs, rhs, .. } => word_contains_call(lhs) || word_contains_call(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            word_contains_call(cond) || word_contains_call(then) || word_contains_call(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            word_contains_call(expr) || word_contains_call(default)
        }
        IrExpr::MethodCall { obj, args, .. } => {
            word_contains_call(obj) || args.iter().any(word_contains_call)
        }
        IrExpr::Array(items) => items.iter().any(word_contains_call),
        _ => false,
    }
}

/// Append one printf argument as an interpolation part, splicing a nested
/// [`IrExpr::Interpolate`] so the fold never nests interpolations (a
/// `"$x\n"`-shaped word arrives as its own Interpolate). `false` when a
/// literal part carries the `\u{1}` glob sentinel — the fold must not bake
/// the EXPAND-me instruction into output text.
fn push_interp_part(parts: &mut Vec<InterpPart>, e: &IrExpr) -> bool {
    match e {
        IrExpr::Interpolate(ps) => {
            for p in ps {
                match p {
                    InterpPart::Lit(t) if t.contains('\u{1}') => return false,
                    _ => parts.push(p.clone()),
                }
            }
        }
        other => parts.push(InterpPart::Expr(Box::new(other.clone()))),
    }
    true
}

/// Can this format's escapes TRUNCATE the output (`\c`) or MINT a `%`
/// byte (`\xHH`, `\0nnn`)? Both break [`printf_s_split`], which reads the
/// RAW format: a minted `%` would be a conversion the split cannot see
/// (while the engine parses it), and a `\c` would drop the tail the split
/// already committed to emitting.
fn printf_escape_can_mint_conversion(fmt: &str) -> bool {
    let ch: Vec<char> = fmt.chars().collect();
    let mut i = 0usize;
    while i + 1 < ch.len() {
        if ch[i] != '\\' {
            i += 1;
            continue;
        }
        if matches!(ch[i + 1], 'c' | 'x' | '0'..='7') {
            return true;
        }
        i += 2;
    }
    false
}

/// Split a printf format into literal chunks separated by PLAIN `%s`
/// conversions — one chunk per argument, so `chunks.len() - 1` is the
/// specifier count. The conversion grammar mirrors `printf_const_eval`
/// line for line (flags `-+ 0#`, width, `.precision`, the `lhLzjt` length
/// modifiers, a trailing bare `%` emitted literally, `%%` collapsed to
/// `%`); anything that is not an unpadded `%s` REFUSES, which sends the
/// statement back to the runtime engine.
fn printf_s_split(fmt: &str) -> Option<Vec<String>> {
    let ch: Vec<char> = fmt.chars().collect();
    let mut chunks: Vec<String> = vec![String::new()];
    let mut i = 0usize;
    while i < ch.len() {
        let c = ch[i];
        if c == '\\' && i + 1 < ch.len() {
            // copied verbatim: the caller already refused the escapes that
            // could shift conversions, the rest are evaluated per chunk
            chunks.last_mut().unwrap().push(c);
            chunks.last_mut().unwrap().push(ch[i + 1]);
            i += 2;
            continue;
        }
        if c == '%' && i + 1 < ch.len() {
            let mut j = i + 1;
            let mut plain = true;
            while j < ch.len() && "-+ 0#".contains(ch[j]) {
                plain = false;
                j += 1;
            }
            while j < ch.len() && ch[j].is_ascii_digit() {
                plain = false;
                j += 1;
            }
            if j < ch.len() && ch[j] == '.' {
                plain = false;
                j += 1;
                while j < ch.len() && ch[j].is_ascii_digit() {
                    j += 1;
                }
            }
            while j < ch.len() && "lhLzjt".contains(ch[j]) {
                plain = false;
                j += 1;
            }
            if j >= ch.len() {
                // the engine emits a bare `%` and stops — literal tail
                chunks.last_mut().unwrap().push('%');
                break;
            }
            let conv = ch[j];
            if conv == '%' {
                chunks.last_mut().unwrap().push('%');
                i = j + 1;
                continue;
            }
            if !plain || conv != 's' {
                return None;
            }
            chunks.push(String::new());
            i = j + 1;
            continue;
        }
        chunks.last_mut().unwrap().push(c);
        i += 1;
    }
    Some(chunks)
}

/// Compile-time evaluation of `printf`, byte-faithful to the runtime
/// `__sh_printf` engine (`rust_backend.rs`'s `"printf"` helper template).
///
/// The port mirrors the engine's control flow LINE FOR LINE — same escape
/// table scanned inline in the format (`\c` truncates the WHOLE output,
/// `\0nnn`/`\xHH` nibble counting), same flags/width/precision parse,
/// same length-modifier skip, same width padding, same format-reuse loop
/// (bash re-applies the format until the arguments are consumed; a round
/// that consumes no argument stops it, so a conversion-free format prints
/// exactly once). The lockstep is the invariant: the equivalence gates
/// compare the fold's output against what the engine would have written,
/// and every construct the port cannot reproduce refuses — the statement
/// then stays on the engine byte-identically.
///
/// REFUSALS (each returns `None`, keeping the statement on the runtime
/// path):
///   * float conversions `f F e E g G` — the engine routes them through
///     `__sh_atof` + Rust `format!("{:.*}")`, a rounding contract this
///     fold will not re-derive;
///   * any other conversion outside `s b q c d i u o x X %` — the engine's
///     `_` arm prints the raw argument for an unknown conversion, a
///     contract nothing should depend on and this fold will not guess.
fn printf_const_eval(fmt: &str, args: &[String]) -> Option<String> {
    let ch: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut ai = 0usize;
    loop {
        let start_ai = ai;
        let mut i = 0usize;
        while i < ch.len() {
            let c = ch[i];
            if c == '\\' && i + 1 < ch.len() {
                i += 1;
                match ch[i] {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'a' => out.push('\x07'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0c'),
                    'v' => out.push('\x0b'),
                    'e' => out.push('\x1b'),
                    '\\' => out.push('\\'),
                    'c' => return Some(out),
                    '0'..='7' => {
                        let mut n = 0u32;
                        let mut k = 0;
                        while k < 3
                            && i + k < ch.len()
                            && ch[i + k].is_ascii_digit()
                            && ch[i + k] <= '7'
                        {
                            n = n * 8 + ch[i + k].to_digit(8).unwrap();
                            k += 1;
                        }
                        out.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                        i += k - 1;
                    }
                    'x' => {
                        let mut n = 0u32;
                        let mut k = 0;
                        while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                            n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap();
                            k += 1;
                        }
                        if k > 0 {
                            out.push(char::from_u32(n).unwrap_or('\0'));
                            i += k;
                        } else {
                            out.push('x');
                        }
                    }
                    _ => {
                        out.push('\\');
                        out.push(ch[i]);
                    }
                }
                i += 1;
                continue;
            }
            if c == '%' && i + 1 < ch.len() {
                let mut j = i + 1;
                let mut flags = String::new();
                while j < ch.len() && "-+ 0#".contains(ch[j]) {
                    flags.push(ch[j]);
                    j += 1;
                }
                let mut width = String::new();
                while j < ch.len() && ch[j].is_ascii_digit() {
                    width.push(ch[j]);
                    j += 1;
                }
                let mut prec: Option<String> = None;
                if j < ch.len() && ch[j] == '.' {
                    j += 1;
                    let mut p = String::new();
                    while j < ch.len() && ch[j].is_ascii_digit() {
                        p.push(ch[j]);
                        j += 1;
                    }
                    prec = Some(p);
                }
                // C length modifiers (`%lld`, `%zu`, `%hhd`) are skipped:
                // the conversion letter is what selects the arm.
                while j < ch.len() && "lhLzjt".contains(ch[j]) {
                    j += 1;
                }
                if j >= ch.len() {
                    out.push('%');
                    break;
                }
                let conv = ch[j];
                i = j + 1;
                if conv == '%' {
                    out.push('%');
                    continue;
                }
                if !matches!(conv, 's' | 'b' | 'q' | 'c' | 'd' | 'i' | 'u' | 'o' | 'x' | 'X') {
                    return None;
                }
                let arg = args.get(ai).cloned().unwrap_or_default();
                ai += 1;
                let w: usize = width.parse().unwrap_or(0);
                let left = flags.contains('-');
                let zero = flags.contains('0');
                let p: Option<usize> = prec.as_ref().map(|s| s.parse().unwrap_or(0));
                let mut piece = String::new();
                match conv {
                    's' => {
                        piece = arg.clone();
                        if let Some(p) = p {
                            piece = piece.chars().take(p).collect();
                        }
                    }
                    'b' => {
                        piece = echo_esc(&arg);
                        if let Some(p) = p {
                            piece = piece.chars().take(p).collect();
                        }
                    }
                    'q' => piece = printf_q(&arg),
                    'c' => piece = arg.chars().next().map(|c| c.to_string()).unwrap_or_default(),
                    'd' | 'i' => piece = printf_atoi(&arg).to_string(),
                    'u' => piece = printf_atou(&arg).to_string(),
                    'o' => piece = format!("{:o}", printf_atou(&arg)),
                    'x' => piece = format!("{:x}", printf_atou(&arg)),
                    'X' => piece = format!("{:X}", printf_atou(&arg)),
                    _ => unreachable!("refused above"),
                }
                if w > piece.chars().count() {
                    let pad = w - piece.chars().count();
                    if left {
                        piece = format!("{}{}", piece, " ".repeat(pad));
                    } else if zero {
                        piece = format!("{}{}", "0".repeat(pad), piece);
                    } else {
                        piece = format!("{}{}", " ".repeat(pad), piece);
                    }
                }
                out.push_str(&piece);
                continue;
            }
            out.push(c);
            i += 1;
        }
        if ai >= args.len() || ai == start_ai {
            break;
        }
    }
    Some(out)
}

/// `printf %q` quoting — port of the runtime `__sh_q_printf` helper
/// (lockstep with the engine template; see [`printf_const_eval`]).
fn printf_q(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let mut ansi = false;
    for c in s.chars() {
        if matches!(c, '\0'..='\x1f' | '\x7f') {
            ansi = true;
        }
    }
    if ansi {
        let mut body = String::new();
        for c in s.chars() {
            match c {
                '\n' => body.push_str("\\n"),
                '\t' => body.push_str("\\t"),
                '\r' => body.push_str("\\r"),
                '\'' => body.push_str("\\'"),
                '\\' => body.push_str("\\\\"),
                c if matches!(c, '\0'..='\x1f' | '\x7f') => {
                    body.push_str(&format!("\\x{:02x}", u32::from(c)))
                }
                _ => body.push(c),
            }
        }
        return format!("$'{}'", body);
    }
    let mut o = String::new();
    for c in s.chars() {
        match c {
            ' ' | '\'' | '\\' | '"' | '$' | '`' => {
                o.push('\\');
                o.push(c);
            }
            _ => o.push(c),
        }
    }
    o
}

/// Port of the runtime `__sh_atoi` helper (trim, `0x`/`0X` radix
/// prefix, else base-10, `unwrap_or(0)` fallthrough) — lockstep with the
/// engine template; see [`printf_const_eval`].
fn printf_atoi(s: &str) -> i64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<i64>().unwrap_or(0)
}

/// Port of the runtime `__sh_atou` helper; see [`printf_atoi`].
fn printf_atou(s: &str) -> u64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<u64>().unwrap_or(0)
}

/// `Some(Output)` when `st` is a constant `echo` — see [`fold_to_output`].
fn constant_echo(st: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "echo" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    // an all-literal echo folds to a precomputed string; a single
    // INTEGER-valued word folds to an Output carrying the value (so the
    // print path formats the number directly instead of round-tripping
    // through `.to_string()`).
    fold_to_output(words).or_else(|| fold_numeric_echo(args))
}

/// The constant-echo fold: join the (all-literal) words, apply the
/// leading `-n`/`-e`/`-E` flags, append the trailing newline. `None`
/// when any word is not a compile-time constant (so the caller leaves
/// the statement for the runtime helper).
fn fold_to_output(words: &[IrExpr]) -> Option<IrStmt> {
    let mut nl = true;
    let mut esc = false;
    let mut idx = 0;
    while let Some(IrExpr::Str(f, _)) = words.get(idx) {
        match f.as_str() {
            "-n" => nl = false,
            "-e" => esc = true,
            "-E" => esc = false,
            "--" => {
                idx += 1;
                break;
            }
            _ => break,
        }
        idx += 1;
    }
    let mut text = String::new();
    let mut first = true;
    for w in &words[idx..] {
        let t = const_word_text(w)?;
        if !first {
            text.push(' ');
        }
        first = false;
        text.push_str(&t);
    }
    if esc {
        text = echo_esc(&text);
    }
    // `Output.newline` carries the trailing newline (its documented
    // meaning — every backend appends it), so the folded text holds only
    // the joined words. Baking `\n` into the text AND setting the flag
    // would double it.
    Some(IrStmt::Output {
        value: IrExpr::Str(text, StrStyle::DoubleQuoted),
        newline: nl,
        target: None,
    })
}

/// The DYNAMIC twin of [`fold_to_output`] — the same native `Output`,
/// built as `InterpPart::Lit` runs around the word expressions.
///
/// `echo` was lowered natively only when EVERY word was a compile-time
/// constant, so `echo "count: $n"` kept reaching the backends as
/// `Expr(Call echo)` and each paid for the word-printing helper at render
/// time — `__sh_print_words` (22 lines) in 87 of 563 emitted Rust files,
/// the `sh2.builtin("echo", …)` dispatch in ESTree, `System.out` vs a
/// runtime call in Java. `IrStmt::Output` is the node every backend
/// already renders natively, so folding the dynamic form here removes the
/// helper in every backend at once — the shared twin of what each
/// renderer would otherwise re-derive.
///
/// Soundness — every refusal leaves the statement byte-identical on the
/// runtime path:
///   * `-e`: the escape decoding happens at PRINT time and `Output`
///     carries no esc flag (the all-literal fold owns `-e`, it decodes
///     now with the same [`echo_esc`] table);
///   * a word that expands to SEVERAL words — [`arg_expands_to_many_words`]
///     (an array, `${a[@]}`, a `split`/`words` result): the helper joins
///     N words, one `Interp` value cannot say the same thing about the
///     separators;
///   * a bare `$(…)`: the word printer IFS-splits an UNQUOTED capture
///     before the space-join (`split` then `join(" ")` collapses tabs and
///     newlines that a single word would keep). Quoted `"$(…)"` arrives
///     inside an `Interpolate` and is one word there, so it still folds;
///   * `Bool`: `words_expr` renders it as an EMPTY word list (prints just
///     the newline), which a formatted value would spell `true`/`false`;
///   * a literal carrying the `\u{1}` glob/byte sentinel — the renderers
///     decode those on the print path, baking the instruction into the
///     text would print it instead;
///   * flags parsed exactly like [`fold_to_output`]: only leading LITERAL
///     words are flags, so `echo "-n" $x` keeps its words.
fn fold_dynamic_to_output(words: &[IrExpr]) -> Option<IrStmt> {
    let mut nl = true;
    let mut esc = false;
    let mut idx = 0;
    while let Some(IrExpr::Str(f, _)) = words.get(idx) {
        match f.as_str() {
            "-n" => nl = false,
            "-e" => esc = true,
            "-E" => esc = false,
            "--" => {
                idx += 1;
                break;
            }
            _ => break,
        }
        idx += 1;
    }
    if esc {
        return None;
    }
    let rest = &words[idx..];
    if rest.is_empty() {
        return None;
    }
    // an all-literal `echo` is [`fold_to_output`]'s shape (it also owns
    // `-e` decoding); this fold exists for the mixed/dynamic case
    if rest.iter().all(|w| matches!(w, IrExpr::Str(_, _))) {
        return None;
    }
    if rest.iter().any(arg_expands_to_many_words) {
        return None;
    }
    // A word carrying a CALL is refused, mirroring the `WriteFile` fold.
    // The echo path does more than produce text: it renders each word in
    // command-word context, and two things ride along with that which a
    // native `Output` does not reproduce.
    //
    //  * A `$((…))` word is handed to the backend's own evaluator, which
    //    may arm a failure guard — Rust's `__SH_ARITH_WORD_FAIL`, consumed
    //    by `__sh_print_words` to suppress the WHOLE command, and Perl's
    //    `$__SH2_AE` — and `probe_str`s the positionals it reads.
    //  * That same probe is what HOMES the enclosing function's
    //    parameters: it runs the render and records whether each
    //    positional was read as a number or as text. Folding the echo
    //    changes what the render records. Measured: `small_factors`
    //    (t89/t90) went `p0: String` → `p0: i64`, the call site then
    //    wrapped the argument in `.trim().parse::<i64>().unwrap_or(0)`,
    //    and a 2^64+1 BigInt parsed to 0 — py-rust-gate 96/103 vs 98/103
    //    at HEAD, wrong factor lists.
    //
    // Literal and interpolated VARIABLE text (the common `echo "x=$v"`
    // shape, and the whole reason this fold exists) carries no call and
    // still folds. Refuse > guess.
    if rest.iter().any(word_contains_call) {
        return None;
    }
    let mut parts: Vec<InterpPart> = Vec::with_capacity(rest.len() * 2);
    for (i, w) in rest.iter().enumerate() {
        if i > 0 {
            parts.push(InterpPart::Lit(" ".to_string()));
        }
        match w {
            IrExpr::Str(t, _) => {
                if t.contains('\u{1}') {
                    return None;
                }
                parts.push(InterpPart::Lit(t.clone()));
            }
            // unquoted `$(…)`: IFS-split before the join (see above)
            IrExpr::Capture { .. } => return None,
            // an empty word list, not a word — see above
            IrExpr::Bool(_) => return None,
            other => {
                if !push_interp_part(&mut parts, other) {
                    return None;
                }
            }
        }
    }
    Some(IrStmt::Output {
        value: parts_value(parts),
        newline: nl,
        target: None,
    })
}

/// Shape-match an `echo` whose words are not all literal and lower it —
/// the shared entry point [`fold_narrow`] calls after
/// [`constant_echo`] declines.
fn dynamic_echo(st: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "echo" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    fold_dynamic_to_output(words)
}

/// Collapse interpolation parts to the narrowest value node: no parts is
/// the empty literal, ONE part needs no `Interpolate` wrapper (a lone
/// expression prints as itself, a lone literal is a plain `Str`).
fn parts_value(parts: Vec<InterpPart>) -> IrExpr {
    match parts.len() {
        0 => IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
        1 => match parts.into_iter().next().unwrap() {
            InterpPart::Lit(s) => IrExpr::Str(s, StrStyle::DoubleQuoted),
            InterpPart::Expr(e) => *e,
        },
        _ => IrExpr::Interpolate(parts),
    }
}

/// Fold a single-word `echo` whose word is an INTEGER by construction
/// into a native `Output` carrying the value itself.
///
/// `echo <int-expr>` is a print of a number: leaving it an exec call makes
/// every renderer stringify first (`__sh_len(&s).to_string()`), only for
/// the print to format it back. `Output { value, newline: true }` lets each
/// backend print the value directly (`println!("{}", __sh_len(&s))`).
///
/// Conservative: exactly `echo` with ONE word (a single word has no
/// separator, so word joining cannot matter) and a value that is numeric
/// BY CONSTRUCTION — a bare `Int`/`Arith`, or the int-returning builtins
/// (`param("len", …)`, `arrayLen`). Anything else refuses; the string path
/// is correct, just noisier.
fn fold_numeric_echo(args: &[IrExpr]) -> Option<IrStmt> {
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "echo" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    let [word] = words.as_slice() else {
        return None;
    };
    if !numeric_word(word) {
        return None;
    }
    Some(IrStmt::Output {
        value: word.clone(),
        newline: true,
        target: None,
    })
}

/// Is this word a number by construction (so a print can format it with no
/// stringification round-trip)? A bare `Int`/`Arith`, and the builtins that
/// return an integer.
pub fn numeric_word(w: &IrExpr) -> bool {
    match w {
        IrExpr::Int(_) | IrExpr::Arith(_) => true,
        IrExpr::Call { func, args } => match func.as_str() {
            "arrayLen" => true,
            // `param(op, …)`: only the LENGTH op yields an integer
            "param" => matches!(args.first(), Some(IrExpr::Str(op, _)) if op == "len"),
            _ => false,
        },
        _ => false,
    }
}

/// The compile-time text of a word, or `None` when it involves any
/// expansion/glob. A bare `Str` and an `Interpolate` whose parts are ALL
/// literals both fold (a quoted `"s1"` arrives as a one-part
/// Interpolate).
fn const_word_text(w: &IrExpr) -> Option<String> {
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

/// bash `echo -e` backslash unescaping (`\n \t \r \a \b \f \v \e \\ \c
/// \0NNN \xHH`). This is the SHARED definition of the fold's `-e`
/// semantics: the `--shir` contract and the backends' runtime
/// unescapers must agree with it (`\c` truncates; an incomplete escape
/// is left as-is).
pub fn echo_esc(s: &str) -> String {
    let mut o = String::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            i += 1;
            match ch[i] {
                'n' => o.push('\n'),
                't' => o.push('\t'),
                'r' => o.push('\r'),
                'a' => o.push('\x07'),
                'b' => o.push('\x08'),
                'f' => o.push('\x0c'),
                'v' => o.push('\x0b'),
                'e' => o.push('\x1b'),
                '\\' => o.push('\\'),
                'c' => return o,
                '0'..='7' => {
                    let mut n = 0;
                    let mut k = 0;
                    while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                        n = n * 8 + (ch[i + k] as u32 - '0' as u32);
                        k += 1;
                    }
                    o.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                    i += k - 1;
                }
                'x' => {
                    let mut n = 0;
                    let mut k = 0;
                    while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                        n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap();
                        k += 1;
                    }
                    if k > 0 {
                        o.push(char::from_u32(n).unwrap_or('\0'));
                        i += k;
                    } else {
                        o.push('x');
                    }
                }
                c => {
                    o.push('\\');
                    o.push(c);
                }
            }
            i += 1;
        } else {
            o.push(ch[i]);
            i += 1;
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrRedirect;

    fn words(v: &[&str]) -> Vec<IrExpr> {
        v.iter()
            .map(|s| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted))
            .collect()
    }

    /// `Redirect { inner, [one fd-1 w/a spec] }` — the shape the
    /// writefile fold inspects.
    fn redirect(mode: &str, fd: Option<i32>, target: IrExpr, inner: Vec<IrStmt>) -> IrStmt {
        IrStmt::Redirect {
            inner,
            redirects: vec![IrRedirect {
                fd,
                mode: mode.to_string(),
                target,
                interpolate: false,
            }],
        }
    }

    fn out(text: &str, newline: bool) -> IrStmt {
        IrStmt::Output {
            value: IrExpr::Str(text.to_string(), StrStyle::DoubleQuoted),
            newline,
            target: None,
        }
    }

    fn path(name: &str) -> IrExpr {
        IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted)
    }

    /// The whole shape: `echo "hi" > f` first folds its producer to a
    /// native `Output`, then the enclosing redirect becomes a native
    /// `WriteFile`.
    #[test]
    fn echo_redirect_folds_end_to_end() {
        let echo = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["hi"])),
            ],
        });
        let mut stmts = vec![redirect("w", Some(1), path("f"), vec![echo])];
        fold_narrow(&mut stmts);
        fold_wide(&mut stmts, false);
        match &stmts[0] {
            IrStmt::WriteFile {
                path: p,
                content,
                append,
            } => {
                assert_eq!(*p, path("f"));
                assert!(!append);
                assert_eq!(
                    *content,
                    IrExpr::Str("hi\n".to_string(), StrStyle::DoubleQuoted),
                    "the joined words plus the Output newline"
                );
            }
            other => panic!("expected WriteFile, got {other:?}"),
        }
    }

    /// `>>` maps to the node's `append` flag.
    #[test]
    fn append_mode_flags_append() {
        let mut st = redirect("a", Some(1), path("f"), vec![out("x", true)]);
        assert!(redirected_output_to_writefile(&mut st));
        let IrStmt::WriteFile { append, content, .. } = st else {
            panic!("expected WriteFile")
        };
        assert!(append);
        assert_eq!(content, IrExpr::Str("x\n".to_string(), StrStyle::DoubleQuoted));
    }

    /// `echo -n` — the `Output` already carries `newline: false`, so the
    /// write must NOT bake in a trailing `\n`.
    #[test]
    fn output_without_newline_stays_without_one() {
        let mut st = redirect("w", Some(1), path("f"), vec![out("x", false)]);
        assert!(redirected_output_to_writefile(&mut st));
        let IrStmt::WriteFile { content, .. } = st else {
            panic!("expected WriteFile")
        };
        assert_eq!(content, IrExpr::Str("x".to_string(), StrStyle::DoubleQuoted));
    }

    /// A dynamic producer (`printf '%s\n' "$x" > f`) keeps its value and
    /// appends the newline as a second interpolation part — the node
    /// every renderer already spells `"$x"` with.
    #[test]
    fn dynamic_output_keeps_the_expression() {
        let value = IrExpr::Var("x".to_string(), None);
        let st = IrStmt::Output {
            value: value.clone(),
            newline: true,
            target: None,
        };
        let mut st = redirect("w", Some(1), path("f"), vec![st]);
        assert!(redirected_output_to_writefile(&mut st));
        let IrStmt::WriteFile { content, .. } = st else {
            panic!("expected WriteFile")
        };
        assert_eq!(
            content,
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(value)),
                InterpPart::Lit("\n".to_string()),
            ])
        );
    }

    /// A CALL anywhere in the value refuses the fold and leaves the
    /// statement on the redirect machinery: `h() { echo in-h; }; h > file`
    /// rendered through the fold as `fs::write(f, format!("{}\n", h()))`
    /// while `-> String` elision (which walks `Output` values, not
    /// `WriteFile` content) had dropped the callee's return type — rustc
    /// E0277, and the guarded-`arith` / bash-no-open cases in the fold's
    /// doc. The word path keeps rendering it.
    #[test]
    fn call_values_refuse_the_writefile_fold() {
        for value in [
            IrExpr::Call {
                func: "fnValue".to_string(),
                args: vec![
                    IrExpr::Str("h".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            },
            IrExpr::Call {
                func: "arrayLen".to_string(),
                args: vec![IrExpr::Str("a".to_string(), StrStyle::DoubleQuoted)],
            },
            // a call INSIDE the interpolation still refuses
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(IrExpr::Call {
                    func: "arrayLen".to_string(),
                    args: vec![IrExpr::Str("a".to_string(), StrStyle::DoubleQuoted)],
                })),
                InterpPart::Lit("\n".to_string()),
            ]),
        ] {
            let st = IrStmt::Output {
                value,
                newline: true,
                target: None,
            };
            let mut st = redirect("w", Some(1), path("f"), vec![st]);
            assert!(
                !redirected_output_to_writefile(&mut st),
                "call value folded to WriteFile: {st:?}"
            );
            assert!(matches!(st, IrStmt::Redirect { .. }));
        }
    }

    /// Every refusal keeps the statement byte-identical on the old path.
    #[test]
    fn redirect_refusals() {
        // fd 2 — the producer never writes stderr
        let mut st = redirect("w", Some(2), path("f"), vec![out("x", true)]);
        assert!(!redirected_output_to_writefile(&mut st));
        assert!(matches!(st, IrStmt::Redirect { .. }));

        // a read redirect is not a write
        let mut st = redirect("r", Some(0), path("f"), vec![out("x", true)]);
        assert!(!redirected_output_to_writefile(&mut st));

        // `>&1` — an fd-table target, not a path
        let mut st = redirect(
            "w",
            Some(1),
            IrExpr::Str("&1".to_string(), StrStyle::DoubleQuoted),
            vec![out("x", true)],
        );
        assert!(!redirected_output_to_writefile(&mut st));

        // two redirects — folding would drop the sibling
        let two = IrStmt::Redirect {
            inner: vec![out("x", true)],
            redirects: vec![
                IrRedirect {
                    fd: Some(1),
                    mode: "w".to_string(),
                    target: path("f"),
                    interpolate: false,
                },
                IrRedirect {
                    fd: Some(2),
                    mode: "w".to_string(),
                    target: path("g"),
                    interpolate: false,
                },
            ],
        };
        let mut st = two;
        assert!(!redirected_output_to_writefile(&mut st));

        // a non-Output producer (a shell-out that streams its own bytes)
        let mut st = redirect(
            "w",
            Some(1),
            path("f"),
            vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    IrExpr::Str("date".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            })],
        );
        assert!(!redirected_output_to_writefile(&mut st));

        // a raw-byte marker: the print path decodes it, a file write would not
        let mut st = redirect("w", Some(1), path("f"), vec![out("a\u{1}SH2BYTE\u{1}41\u{1}b", true)]);
        assert!(!redirected_output_to_writefile(&mut st));

        // an `Arrow` target is a process substitution (`>(cmd)`), not a path
        let mut st = redirect("w", None, IrExpr::Arrow(vec![]), vec![out("x", true)]);
        assert!(!redirected_output_to_writefile(&mut st));
        assert!(matches!(st, IrStmt::Redirect { .. }));
    }

    #[test]
    fn literal_echo_folds_to_output() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["hello"])),
            ],
        });
        let out = constant_echo(&st).expect("folds");
        match out {
            IrStmt::Output { value, newline, target } => {
                assert!(newline);
                assert_eq!(target, None);
                assert_eq!(value, IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted));
            }
            other => panic!("expected Output, got {other:?}"),
        }
    }

    #[test]
    fn flags_and_join() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["-n", "a", "b"])),
            ],
        });
        let IrStmt::Output { value, newline, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert!(!newline);
        assert_eq!(value, IrExpr::Str("a b".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn dash_e_unescapes() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["-e", "x\\ty"])),
            ],
        });
        let IrStmt::Output { value, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("x\ty".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn expansion_word_does_not_fold() {
        // `echo "$x"` — the word is an Interpolate with an Expr part
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Expr(
                    Box::new(IrExpr::Var("x".to_string(), None)),
                )])]),
            ],
        });
        assert!(constant_echo(&st).is_none());
    }

    #[test]
    fn quoted_literal_interpolate_folds() {
        // `echo "s1"` arrives as a one-part Interpolate
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Lit(
                    "s1".to_string(),
                )])]),
            ],
        });
        let IrStmt::Output { value, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("s1".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn non_echo_exec_does_not_fold() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("printf".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["hi"])),
            ],
        });
        assert!(constant_echo(&st).is_none());
    }

    #[test]
    fn echo_esc_matches_bash_common_cases() {
        assert_eq!(echo_esc("a\\nb"), "a\nb");
        assert_eq!(echo_esc("a\\\\b"), "a\\b");
        assert_eq!(echo_esc("ab\\cde"), "ab"); // \c truncates
        assert_eq!(echo_esc("\\101"), "A"); // octal
    }
}

#[cfg(test)]
mod printf_tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn printf_stmt(words: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    words
                        .iter()
                        .map(|s| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    #[test]
    fn constant_printf_folds_to_output() {
        // the format's `\n` is materialized by printf_const_eval (lockstep
        // with the runtime __sh_printf body); `newline: false` because
        // printf appends no newline of its own.
        let out = constant_printf(&printf_stmt(&["a\\nb\\n"]))
            .expect("a literal zero-arg printf must fold");
        match out {
            IrStmt::Output { value, newline, target } => {
                assert!(!newline);
                assert_eq!(target, None);
                assert_eq!(
                    value,
                    IrExpr::Str("a\nb\n".to_string(), StrStyle::DoubleQuoted)
                );
            }
            other => panic!("expected Output, got {other:?}"),
        }
    }

    #[test]
    fn percent_format_folds() {
        // zero args: the missing `%s` argument is the empty string
        // (the engine's `unwrap_or_default`), so the fold derives the
        // same bytes `__sh_printf` would have printed.
        let out = constant_printf(&printf_stmt(&["%s\\n"])).expect("folds");
        let IrStmt::Output { value, newline, .. } = out else {
            panic!("expected Output")
        };
        assert!(!newline);
        assert_eq!(value, IrExpr::Str("\n".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn format_reuse_rounds() {
        // bash re-applies the format per remaining argument; a round
        // that consumes no argument stops the loop.
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%s", "hi"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted));
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%s-%s", "foo", "bar"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(
            value,
            IrExpr::Str("foo-bar".to_string(), StrStyle::DoubleQuoted)
        );
        // one conversion, three args → three rounds; a conversion-free
        // format prints exactly once (the engine's ai == start_ai stop;
        // bash agrees: `printf 'x\n' y` prints one x)
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["x\\n", "y"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("x\n".to_string(), StrStyle::DoubleQuoted));
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["a%.0s", "1", "2", "3"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("aaa".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn width_and_left_flag_match_the_engine() {
        // the exact 014_ansi_quoting corpus shape: `%-10s` pads right
        let IrStmt::Output { value, .. } = constant_printf(&printf_stmt(&[
            "%-10s %-10s %s\\n",
            "Name",
            "Age",
            "City",
        ]))
        .expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(
            value,
            IrExpr::Str(
                "Name       Age        City\n".to_string(),
                StrStyle::DoubleQuoted
            )
        );
        // right-align (the default) zero-fills only with the `0` flag
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%5d|", "42"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("   42|".to_string(), StrStyle::DoubleQuoted));
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%05d|", "42"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("00042|".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn integer_conversions_port_the_engine_helpers() {
        // %d through the ported atoi: trim, 0x prefix, else base 10,
        // `unwrap_or(0)` — exactly what __sh_atoi returns at run time.
        for (arg, want) in [
            ("42", "42"),
            (" 17 ", "17"),
            ("0x1f", "31"),
            ("abc", "0"),
            ("", "0"),
        ] {
            let IrStmt::Output { value, .. } =
                constant_printf(&printf_stmt(&["%d", arg])).expect("folds")
            else {
                panic!("expected Output")
            };
            assert_eq!(
                value,
                IrExpr::Str(want.to_string(), StrStyle::DoubleQuoted),
                "atoi port diverged for {arg:?}"
            );
        }
        // %x/%X/%o go through atou
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%x", "255"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("ff".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn q_and_b_conversions_port_the_engine_helpers() {
        // %q with control chars → the ANSI-C $'…' quoting (0x0a 0x09)
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%q", "\n\t"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(
            value,
            IrExpr::Str("$'\\n\\t'".to_string(), StrStyle::DoubleQuoted)
        );
        // %q of a plain word backslash-escapes the shell metachars
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%q", "a b"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(
            value,
            IrExpr::Str("a\\ b".to_string(), StrStyle::DoubleQuoted)
        );
        // %b runs the arg through the shared escape table
        let IrStmt::Output { value, .. } =
            constant_printf(&printf_stmt(&["%b", "a\\tb\\n"])).expect("folds")
        else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("a\tb\n".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn float_and_unknown_conversions_refuse() {
        // %f/%e route through __sh_atof + Rust float formatting — a
        // rounding contract the fold will not re-derive
        assert!(constant_printf(&printf_stmt(&["%.2f", "3.14159"])).is_none());
        // an unknown conversion falls to the engine's `_` arm — the fold
        // will not guess what that prints
        assert!(constant_printf(&printf_stmt(&["%m", "x"])).is_none());
    }

    #[test]
    fn leading_dash_format_with_args_refuses() {
        // `-v var fmt` shifts the format position (bash parses printf's
        // own options first) — the runtime path parses it
        assert!(constant_printf(&printf_stmt(&["-v", "VAR", "%s", "x"])).is_none());
        assert!(constant_printf(&printf_stmt(&["--", "%s", "x"])).is_none());
        // but `-v` alone keeps its original single-word refusal
        assert!(constant_printf(&printf_stmt(&["-v"])).is_none());
    }

    #[test]
    fn dynamic_argument_folds_through_the_percent_s_template() {
        // `printf '%s=[%s]\n' n "$v"` — the format is a literal whose
        // conversions are all plain `%s`, so the statement lowers to a
        // native interpolated Output instead of staying on the runtime
        // engine (`printf_s_fold`); the dynamic word becomes an Expr part.
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("%s=[%s]\n".into(), StrStyle::DoubleQuoted),
                    IrExpr::Str("n".into(), StrStyle::DoubleQuoted),
                    IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                        "v".to_string(),
                        None,
                    )))]),
                ]),
            ],
        });
        let IrStmt::Output { value, newline, target } = constant_printf(&st).unwrap() else {
            panic!("expected Output")
        };
        // printf appends no newline of its own — the `\n` is a literal run
        assert!(!newline);
        assert_eq!(target, None);
        let IrExpr::Interpolate(parts) = value else {
            panic!("expected an interpolated value, got {value:?}")
        };
        assert_eq!(
            parts,
            vec![
                InterpPart::Lit("n".to_string()),
                InterpPart::Lit("=[".to_string()),
                InterpPart::Expr(Box::new(IrExpr::Var("v".to_string(), None))),
                InterpPart::Lit("]\n".to_string()),
            ]
        );
    }

    #[test]
    fn one_dynamic_argument_collapses_to_the_bare_expression() {
        // a format with no literal surroundings: `printf '%s' "$v"` —
        // a one-part interpolation renders as the value itself, so the
        // backend prints it with no `format!` wrapper at all
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("%s".into(), StrStyle::DoubleQuoted),
                    IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                        "v".to_string(),
                        None,
                    )))]),
                ]),
            ],
        });
        let IrStmt::Output { value, newline, .. } = constant_printf(&st).unwrap() else {
            panic!("expected Output")
        };
        assert!(!newline);
        assert_eq!(value, IrExpr::Var("v".to_string(), None));
    }

    #[test]
    fn padded_or_numeric_conversions_with_a_dynamic_word_refuse() {
        // a width changes the bytes (`%-10s` pads) and `%d` needs
        // __sh_atoi — neither can be a plain interpolation
        let dyn_word = || {
            IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                "v".to_string(),
                None,
            )))])
        };
        let mk = |fmt: &str| IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str(fmt.into(), StrStyle::DoubleQuoted), dyn_word()]),
            ],
        });
        assert!(constant_printf(&mk("%-10s\n")).is_none(), "width must refuse");
        assert!(constant_printf(&mk("%.3s\n")).is_none(), "precision must refuse");
        assert!(constant_printf(&mk("%d\n")).is_none(), "%d must refuse");
        // …while the plain form folds
        assert!(constant_printf(&mk("%s\n")).is_some());
    }

    #[test]
    fn specifier_argument_count_mismatch_refuses() {
        let dyn_word = || {
            IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                "v".to_string(),
                None,
            )))])
        };
        let mk = |fmt: &str, extra: Vec<IrExpr>| {
            let mut words = vec![IrExpr::Str(fmt.into(), StrStyle::DoubleQuoted), dyn_word()];
            words.extend(extra);
            IrStmt::Expr(IrExpr::Call {
                func: "builtin".into(),
                args: vec![
                    IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                    IrExpr::Array(words),
                ],
            })
        };
        // two specifiers, one argument: bash pads the second with empty
        // AND an extra argument word would never be evaluated here
        assert!(constant_printf(&mk("%s-%s\n", vec![])).is_none());
        // one specifier, two arguments: bash reuses the format per round
        assert!(constant_printf(&mk("%s\n", vec![dyn_word()])).is_none());
    }

    #[test]
    fn conversion_shift_or_truncation_escapes_refuse() {
        // `\c` truncates the WHOLE output and `\x41`/`\101` can MINT a `%`
        // byte the raw-text split would not see as a conversion while the
        // engine parses one — both stay on the runtime path
        let dyn_word = || {
            IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
                "v".to_string(),
                None,
            )))])
        };
        let mk = |fmt: &str| IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str(fmt.into(), StrStyle::DoubleQuoted), dyn_word()]),
            ],
        });
        assert!(constant_printf(&mk("a\\c%s")).is_none(), "\\c must refuse");
        assert!(constant_printf(&mk("\\x25s%s")).is_none(), "\\x must refuse");
        assert!(constant_printf(&mk("\\045s%s")).is_none(), "octal must refuse");
        // a plain `\n` in the format is fine — it stays in its chunk
        assert!(constant_printf(&mk("%s\\n")).is_some());
    }

    #[test]
    fn array_splat_argument_refuses() {
        // `printf '%s ' "${lines[@]}"` (041_process_substitution_mapfile)
        // is ONE IR word but N runtime arguments — bash counts the
        // EXPANDED words against the specifiers and reuses the format per
        // element, so the static count would be lying. The statement stays
        // on the runtime path, which passes the expanded list through.
        let splat = IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("%s ".into(), StrStyle::DoubleQuoted),
                    IrExpr::Call {
                        func: "param".into(),
                        args: vec![
                            IrExpr::Str("slice".into(), StrStyle::DoubleQuoted),
                            IrExpr::Str("lines".into(), StrStyle::DoubleQuoted),
                            IrExpr::Str("@".into(), StrStyle::DoubleQuoted),
                            IrExpr::Str("".into(), StrStyle::DoubleQuoted),
                        ],
                    },
                ]),
            ],
        });
        assert!(constant_printf(&splat).is_none(), "a splat word must refuse");

        // …while a genuinely single-word argument of the same shape folds
        let one_word = IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("%s".into(), StrStyle::DoubleQuoted),
                    IrExpr::Call {
                        func: "param".into(),
                        args: vec![
                            IrExpr::Str("len".into(), StrStyle::DoubleQuoted),
                            IrExpr::Str("lines".into(), StrStyle::DoubleQuoted),
                            IrExpr::Str("".into(), StrStyle::DoubleQuoted),
                        ],
                    },
                ]),
            ],
        });
        assert!(constant_printf(&one_word).is_some(), "a single word must fold");
    }

    #[test]
    fn v_option_refuses() {
        // `-v var fmt` writes to a target, and `-v` alone is a usage
        // error the renderer prints verbatim — leave that path alone
        assert!(constant_printf(&printf_stmt(&["-v"])).is_none());
    }

    #[test]
    fn glob_sentinel_refuses() {
        // \u{1}SH2GLOB\u{1} is the core's EXPAND-me instruction, never
        // literal text (the 064 lesson)
        assert!(constant_printf(&printf_stmt(&["\u{1}SH2GLOB\u{1}"])).is_none());
    }

    #[test]
    fn expansion_word_refuses() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Expr(
                    Box::new(IrExpr::Var("x".to_string(), None)),
                )])]),
            ],
        });
        assert!(constant_printf(&st).is_none());
    }

    #[test]
    fn non_printf_exec_does_not_fold() {
        // `echo` belongs to constant_echo; printf must not claim it
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)]),
            ],
        });
        assert!(constant_printf(&st).is_none());
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    #[test]
    fn numeric_echo_folds() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "arrayLen".into(),
                    args: vec![IrExpr::Str("a".into(), StrStyle::DoubleQuoted)],
                }]),
            ],
        });
        let out = constant_echo(&st).expect("numeric echo must fold");
        match out {
            IrStmt::Output { newline, .. } => assert!(newline),
            other => panic!("not an Output: {other:?}"),
        }
    }
}

#[cfg(test)]
mod traversal_tests {
    use super::*;
    use crate::ir::{IrCaseClause, IrExpr, IrStmt, StrStyle};

    fn const_printf() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("a\\nb\\n".into(), StrStyle::DoubleQuoted)]),
            ],
        })
    }

    fn const_echo() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("hi".into(), StrStyle::DoubleQuoted)]),
            ],
        })
    }

    /// The pipeline-stage shape: `printf 'a\nb\n' | tr …` arrives as an
    /// `Arrow` body carried in EXPRESSION position — the hole the old
    /// statement-only walk could not reach at all.
    #[test]
    fn folds_inside_an_arrow_body() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Arrow(vec![const_printf()]))];
        fold_wide(&mut stmts, true);
        let IrStmt::Expr(IrExpr::Arrow(body)) = &stmts[0] else {
            panic!("arrow wrapper must survive")
        };
        assert!(
            matches!(body[0], IrStmt::Output { newline: false, .. }),
            "printf in an Arrow body must fold: {:?}",
            body[0]
        );
    }

    #[test]
    fn folds_inside_a_lambda_body() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Lambda {
            params: vec![],
            body: vec![const_echo()],
        })];
        fold_wide(&mut stmts, true);
        let IrStmt::Expr(IrExpr::Lambda { body, .. }) = &stmts[0] else {
            panic!("lambda wrapper must survive")
        };
        assert!(matches!(body[0], IrStmt::Output { .. }), "{:?}", body[0]);
    }

    #[test]
    fn folds_inside_a_pipeline_stage() {
        let mut stmts = vec![IrStmt::Pipeline {
            stages: vec![vec![const_printf()], vec![const_echo()]],
            last_output: None,
            capture: None,
            cmd_str: None,
        }];
        fold_wide(&mut stmts, true);
        let IrStmt::Pipeline { stages, .. } = &stmts[0] else {
            panic!("pipeline must survive")
        };
        assert!(matches!(stages[0][0], IrStmt::Output { .. }), "{:?}", stages[0][0]);
        assert!(matches!(stages[1][0], IrStmt::Output { .. }), "{:?}", stages[1][0]);
    }

    #[test]
    fn folds_inside_subshell_background_and_case_clause() {
        let mut stmts = vec![
            IrStmt::Subshell(vec![const_printf()]),
            IrStmt::Background(vec![const_printf()]),
            IrStmt::Case {
                discriminant: IrExpr::Str("v".into(), StrStyle::DoubleQuoted),
                clauses: vec![IrCaseClause {
                    patterns: vec!["*".into()],
                    body: vec![const_printf()],
                }],
            },
        ];
        fold_wide(&mut stmts, true);
        let IrStmt::Subshell(b) = &stmts[0] else { panic!() };
        assert!(matches!(b[0], IrStmt::Output { .. }), "subshell: {:?}", b[0]);
        let IrStmt::Background(b) = &stmts[1] else { panic!() };
        assert!(matches!(b[0], IrStmt::Output { .. }), "background: {:?}", b[0]);
        let IrStmt::Case { clauses, .. } = &stmts[2] else { panic!() };
        assert!(matches!(clauses[0].body[0], IrStmt::Output { .. }), "case: {:?}", clauses[0].body[0]);
    }

    #[test]
    fn folds_inside_try_arms_and_nested_function() {
        let mut stmts = vec![
            IrStmt::Try {
                body: vec![const_printf()],
                excepts: vec![crate::ir::TryExcept {
                    match_expr: None,
                    as_name: None,
                    body: vec![const_printf()],
                }],
                else_body: vec![const_printf()],
                finally_body: vec![const_printf()],
            },
            IrStmt::Function {
                name: "f".into(),
                params: vec![],
                body: vec![const_printf()],
                named_blocks: vec![("end".into(), vec![const_printf()])],
            },
        ];
        fold_wide(&mut stmts, true);
        let IrStmt::Try { body, excepts, else_body, finally_body } = &stmts[0] else { panic!() };
        for (label, b) in [
            ("try", body),
            ("except", &excepts[0].body),
            ("else", else_body),
            ("finally", finally_body),
        ] {
            assert!(matches!(b[0], IrStmt::Output { .. }), "{label}: {:?}", b[0]);
        }
        let IrStmt::Function { body, named_blocks, .. } = &stmts[1] else { panic!() };
        assert!(matches!(body[0], IrStmt::Output { .. }), "fn body: {:?}", body[0]);
        assert!(matches!(named_blocks[0].1[0], IrStmt::Output { .. }), "named block");
    }

    #[test]
    fn reaches_through_call_args_to_an_arrow() {
        // `Call { "pipeline", [Arrow[…], …] }` — expression-position
        // statements behind a call, exactly how the corpus spells a pipe.
        let pipe = IrStmt::Expr(IrExpr::Call {
            func: "pipeline".into(),
            args: vec![
                IrExpr::Arrow(vec![const_printf()]),
                IrExpr::Arrow(vec![const_echo()]),
            ],
        });
        let mut stmts = vec![pipe];
        fold_wide(&mut stmts, true);
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmts[0] else {
            panic!("call must survive")
        };
        assert!(matches!(args[0], IrExpr::Arrow(ref b) if matches!(b[0], IrStmt::Output { .. })), "{:?}", args[0]);
        assert!(matches!(args[1], IrExpr::Arrow(ref b) if matches!(b[0], IrStmt::Output { .. })), "{:?}", args[1]);
    }

    #[test]
    fn redirect_inner_and_target_are_walked() {
        let mut stmts = vec![IrStmt::Redirect {
            inner: vec![const_printf()],
            redirects: vec![crate::ir::IrRedirect {
                fd: None,
                mode: "w".into(),
                target: IrExpr::Arrow(vec![const_echo()]),
                interpolate: false,
            }],
        }];
        fold_wide(&mut stmts, true);
        let IrStmt::Redirect { inner, redirects } = &stmts[0] else { panic!() };
        assert!(matches!(inner[0], IrStmt::Output { .. }), "inner: {:?}", inner[0]);
        assert!(
            matches!(redirects[0].target, IrExpr::Arrow(ref b) if matches!(b[0], IrStmt::Output { .. })),
            "target: {:?}",
            redirects[0].target
        );
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn printf_call() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("a\\n".into(), StrStyle::DoubleQuoted)]),
            ],
        })
    }

    fn echo_call() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("hi".into(), StrStyle::DoubleQuoted)]),
            ],
        })
    }

    /// The production wide pass (`fold_echo = false`): a constant printf
    /// folds in ANY container — including expression-carried ones — while
    /// an `echo` stays an `echo`, because render-side native lowerings
    /// still key on the `Expr(Call{"echo"})` shape there (measured: 4
    /// equivalence regressions when they were moved).
    #[test]
    fn wide_pass_folds_printf_anywhere_but_leaves_echo_alone() {
        let mut stmts = vec![
            IrStmt::Subshell(vec![printf_call(), echo_call()]),
            IrStmt::Expr(IrExpr::Arrow(vec![printf_call(), echo_call()])),
        ];
        fold_wide(&mut stmts, false);
        let IrStmt::Subshell(b) = &stmts[0] else { panic!() };
        assert!(matches!(b[0], IrStmt::Output { .. }), "printf in subshell: {:?}", b[0]);
        assert!(
            matches!(b[1], IrStmt::Expr(IrExpr::Call { .. })),
            "echo in subshell must NOT fold on the wide pass: {:?}",
            b[1]
        );
        let IrStmt::Expr(IrExpr::Arrow(b)) = &stmts[1] else { panic!() };
        assert!(matches!(b[0], IrStmt::Output { .. }), "printf in arrow: {:?}", b[0]);
        assert!(
            matches!(b[1], IrStmt::Expr(IrExpr::Call { .. })),
            "echo in arrow must NOT fold on the wide pass: {:?}",
            b[1]
        );
    }

    /// The legacy narrow pass keeps folding BOTH folds in the containers
    /// it has always visited — the baseline reach must not shrink.
    #[test]
    fn narrow_pass_still_folds_echo_and_printf_in_legacy_containers() {
        let mut stmts = vec![
            echo_call(),
            printf_call(),
            IrStmt::If {
                cond: IrExpr::Str("x".into(), StrStyle::DoubleQuoted),
                then: vec![echo_call()],
                elsifs: vec![],
                else_: vec![printf_call()],
            },
            IrStmt::Block(vec![IrStmt::Redirect {
                inner: vec![echo_call()],
                redirects: vec![],
            }]),
        ];
        fold_narrow(&mut stmts);
        assert!(
            matches!(stmts[0], IrStmt::Output { newline: true, .. }),
            "top-level echo: {:?}",
            stmts[0]
        );
        assert!(matches!(stmts[1], IrStmt::Output { .. }), "top-level printf: {:?}", stmts[1]);
        let IrStmt::If { then, else_, .. } = &stmts[2] else { panic!() };
        assert!(matches!(then[0], IrStmt::Output { .. }), "if-then echo: {:?}", then[0]);
        assert!(matches!(else_[0], IrStmt::Output { .. }), "if-else printf: {:?}", else_[0]);
        let IrStmt::Block(b) = &stmts[3] else { panic!() };
        let IrStmt::Redirect { inner, .. } = &b[0] else { panic!() };
        assert!(matches!(inner[0], IrStmt::Output { .. }), "redirect echo: {:?}", inner[0]);
    }
}
