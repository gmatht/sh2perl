//! Use scopes per name (LISTS.MD follow-up: function-local vecs).
//!
//! Records, for every mentioned name, the scopes containing mentions
//! and whether any mention sits in a *risky* position. A list array is
//! local to function F iff every mention is in scope F and none is
//! risky. Anything else (multiple scopes, top-level/main mentions,
//! risky positions) keeps global storage.
//!
//! Risky = positions that may execute outside the owning scope or inside
//! a file-scope helper: `Capture`, `Pipeline`, `Redirect`,
//! `Background`, `Subshell`, `Arrow`, `ArrayComp`, `Lambda` subtrees
//! (deferred/out-of-order/file-scope-helper execution), test-dispatch
//! calls (`test`, `[` — may shell-fallback into a helper), and opaque
//! `Ext` payloads. Everything else (assigns, plain calls, loop/if
//! bodies, loop conditions, output, returns) renders inline in the
//! owning scope.
//!
//! Matches are exhaustive (no wildcard arms) so a new IR variant breaks
//! compilation instead of silently under-attributing (an unattributed
//! use would wrongly narrow the scope set toward local).

use std::collections::{HashMap, HashSet};

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt};

/// Use scope: top-level/main statements, a named function, or a named sub.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Scope {
    Main,
    Func(String),
}

/// One attributed mention.
#[derive(Clone, Debug)]
pub struct Use {
    pub scope: Scope,
    pub risky: bool,
    /// The mention is a *write* (an assignment target / capture target /
    /// `VAR=x` envar / builtin `name=value` head) rather than a read.
    pub write: bool,
    /// The mention sits in a top-level statement of its scope (not inside
    /// an if/loop/block/pipeline body). Pass-2 locals defer their `let`
    /// to the first write, which is only sound for a top-level write.
    pub top: bool,
}

/// All mentions per name (defs, reads, name-arg positions, `$` refs).
pub fn use_scopes(prog: &IrProgram) -> HashMap<String, Vec<Use>> {
    let mut out: HashMap<String, Vec<Use>> = HashMap::new();
    let mut defined: HashSet<String> = HashSet::new();
    for st in &prog.stmts {
        if let IrStmt::Function { name, .. } = st {
            defined.insert(name.clone());
        }
    }
    for sub in &prog.subs {
        defined.insert(sub.name.clone());
    }
    let mut w = Walker {
        out: &mut out,
        defined: &defined,
        scope: Scope::Main,
        risky: false,
        top: true,
    };
    for st in &prog.stmts {
        w.top_stmt(st);
    }
    for sub in &prog.subs {
        // subs are their own scope (whether or not the backend renders
        // them as functions, sharing a name across scopes must not
        // narrow anything toward local)
        let save = w.scope.clone();
        w.scope = Scope::Func(sub.name.clone());
        for st in &sub.body {
            w.top_stmt(st);
        }
        w.scope = save;
    }
    out
}

struct Walker<'a> {
    out: &'a mut HashMap<String, Vec<Use>>,
    defined: &'a HashSet<String>,
    scope: Scope,
    risky: bool,
    top: bool,
}

impl<'a> Walker<'a> {
    fn mention(&mut self, name: &str) {
        self.push(name, false);
    }

    /// A write mention (assignment target / capture / `VAR=x` word).
    fn mention_write(&mut self, name: &str) {
        self.push(name, true);
    }

    fn push(&mut self, name: &str, write: bool) {
        // binding positions never reach here (callers skip them);
        // everything recorded is a genuine mention
        self.out.entry(name.to_string()).or_default().push(Use {
            scope: self.scope.clone(),
            risky: self.risky,
            write,
            top: self.top,
        });
    }

    fn mentions_str(&mut self, text: &str) {
        // Command-WORD rule: exact identifier match (a bare name argument)
        // and a `name=value` head (a builtin write word) — plus `$`
        // references. Conservative: a literal that merely LOOKS like a
        // mention (`echo factors` printing the word) only widens toward
        // global, never miscompiles.
        str_mention_all(text, &mut |v| self.mention(v));
    }

    /// A string literal in VALUE position (`a = "a"`, `x="b"`). Only its
    /// `$` references are reads; the bare text is DATA, so it must NOT be
    /// matched as an identifier mention. Matching it would record a phantom
    /// read of the assigned name BEFORE the write (`a = "a"` would mention
    /// `a` first), and the "first mention is a write" rule that decides a
    /// pass-2 local would then refuse the name — leaving a thread_local for
    /// a program as simple as t28's `a = "a"; b = "b"; if a == "a" and …`.
    /// A string in VALUE position (`a = "a"`, `x = "$y"`, the substituted
    /// `echo "$x"` operand).
    ///
    /// A `$`-reference is a read — and so is a BARE identifier, because the
    /// frontends hand some operands over already substituted (`"$x"` arrives
    /// as `x`). What must NOT be reported is the plain-value case: `a = "a"`
    /// would otherwise mention `a`, and that phantom read would precede the
    /// write and defeat the "first mention is a write" rule that decides a
    /// pass-2 local (t28's `a = "a"; b = "b"; if a == "a" and …`).
    ///
    /// So: mention an identifier only when it is NOT the text of a literal
    /// being compared/bound — i.e. when the value looks like a reference
    /// (`$v` present) or a `name=value` binding head. A bare word that is
    /// neither is data.
    fn mentions_value_str_style(&mut self, text: &str, quoted: bool) {
        // a `name=value` head always names the variable being bound
        if let Some((head, _)) = text.split_once('=') {
            if is_ident_word(head) {
                self.mention(head);
            }
        }
        for v in dollar_vars(text) {
            self.mention(&v);
        }
        // A bare identifier in VALUE position is a read when it is a
        // SUBSTITUTED reference (`"$x"` reaches here as the text `x`), and
        // DATA when the source quoted it literally (`a = "a"`). The two are
        // indistinguishable from the text alone, so report the mention and
        // let the Assign arm's ORDER rule decide: it records the target's
        // write BEFORE walking a literal RHS, so a literal `a = "a"` still
        // has the write as its first mention (see `stmt_inner`).
        if is_ident_word(text) {
            self.mention(text);
        }
    }

    fn with_scope(&mut self, scope: Scope, body: impl FnOnce(&mut Self)) {
        let save = self.scope.clone();
        self.scope = scope;
        body(self);
        self.scope = save;
    }

    fn with_risky(&mut self, body: impl FnOnce(&mut Self)) {
        let save = self.risky;
        self.risky = true;
        body(self);
        self.risky = save;
    }

    /// A nested statement (inside an if/loop/block/…): mentions are not
    /// top-level for the enclosing scope.
    fn stmt(&mut self, st: &IrStmt) {
        let save = self.top;
        self.top = false;
        self.stmt_inner(st);
        self.top = save;
    }

    /// A top-level statement of the current scope.
    fn top_stmt(&mut self, st: &IrStmt) {
        let save = self.top;
        self.top = true;
        self.stmt_inner(st);
        self.top = save;
    }

    fn stmt_inner(&mut self, st: &IrStmt) {
        match st {
            IrStmt::Ext(n) => {
                // opaque payload: assume a mention is hidden (forces
                // global), but still descend into visible children
                for c in n.children() {
                    self.stmt(c);
                }
            }
            IrStmt::Output { value, .. } => self.expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                self.expr(path);
                self.expr(content);
            }
            IrStmt::Assign { targets, expr, .. } => {
                // A LITERAL RHS is DATA, not a reference: record the write
                // first so `a = "a"` does not open with a phantom read of
                // `a` (that would defeat the "first mention is a write" rule
                // that decides a pass-2 local — t28). A COMPUTED RHS
                // (`x = $x`, `x = $((x+1))`) genuinely reads the OLD value,
                // so it is walked first as before.
                let literal_rhs = matches!(
                    expr,
                    IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Bool(_)
                );
                if !literal_rhs {
                    self.expr(expr);
                }
                for t in targets {
                    // baked subscript (`arr[$i]`): the base is written,
                    // `$i` inside is read
                    let base = t.var.split('[').next().unwrap_or(&t.var);
                    if !base.is_empty() {
                        self.mention_write(base);
                    }
                    // a baked subscript (`arr[$i]`) reads `$i`; a plain
                    // name was already recorded above
                    if t.var != base {
                        self.mentions_str(&t.var);
                    }
                    for ix in &t.indices {
                        self.expr(ix);
                    }
                }
                if literal_rhs {
                    self.expr(expr);
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                // `vars` are bindings, not mentions — skip
                let _ = vars;
                if let Some(e) = init {
                    self.expr(e);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                // array definition: a genuine mention (storage op)
                self.mention(var);
                for e in elements {
                    self.expr(e);
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                self.expr(cond);
                for b in then.iter().chain(else_) {
                    self.stmt(b);
                }
                for (c, b) in elsifs {
                    self.expr(c);
                    for stm in b {
                        self.stmt(stm);
                    }
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body } => {
                for b in body.iter().chain(else_body).chain(finally_body) {
                    self.stmt(b);
                }
                for e in excepts {
                    if let Some(m) = &e.match_expr {
                        self.expr(m);
                    }
                    for b in &e.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::For { iter, body, .. } => {
                // loop var is a binding — skip; iter is read
                self.expr(iter);
                for b in body {
                    self.stmt(b);
                }
            }
            IrStmt::ForInit { init, cond, step, body } => {
                for b in init.iter().chain(step).chain(body) {
                    self.stmt(b);
                }
                self.expr(cond);
            }
            IrStmt::Continue => {}
            IrStmt::Break => {}
            IrStmt::While { cond, body } => {
                self.expr(cond);
                for b in body {
                    self.stmt(b);
                }
            }
            IrStmt::DoWhile { body, cond, .. } => {
                for b in body {
                    self.stmt(b);
                }
                self.expr(cond);
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => self.expr(expr),
            IrStmt::Exec { cmd, args, capture, redirects, env } => {
                // `let`/`eval` args are arithmetic/code: every bare
                // identifier they touch is a potential mention
                // (conservative — widens toward global, never narrows).
                if let IrExpr::Str(cname, _) = cmd {
                    if cname == "let" || cname == "eval" {
                        for a in args {
                            if let IrExpr::Str(t, _) = a {
                                for v in all_idents(t) {
                                    self.mention(&v);
                                }
                            }
                        }
                    }
                }
                // Call-argument dispatch shared with Call expressions:
                // the command word is the head, args are the actuals.
                let head: Option<&str> = match cmd {
                    IrExpr::Str(cname, _) => Some(cname),
                    _ => {
                        self.expr(cmd);
                        None
                    }
                };
                // NOTE: a None head still walks `cmd` above (it may read
                // vars); actuals go fully risky (dynamic dispatch) — except
                // the decl family (`local x=...`), whose `name=` heads bind
                // in the current scope (see `walk_decl_args`).
                if matches!(head, Some("local" | "declare" | "typeset" | "export" | "readonly")) {
                    self.walk_decl_args(args);
                } else {
                    self.dispatch_args(head, args);
                }
                if let Some(c) = capture {
                    // capture target is a binding... but a bare-name
                    // capture (`capture=v`) writes v — record as mention
                    // (conservative; writes narrow scope sets only by
                    // adding the current scope, never removing)
                    self.mention(c);
                }
                for r in redirects {
                    self.with_risky(|w| w.expr(r));
                }
                for (v, e) in env {
                    // `VAR=x cmd`: V is written in this scope
                    self.mention_write(v);
                    self.expr(e);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                self.with_risky(|w| {
                    for stage in stages {
                        for b in stage {
                            w.stmt(b);
                        }
                    }
                });
            }
            IrStmt::Return(e) | IrStmt::Exit(e) => {
                if let Some(x) = e {
                    self.expr(x);
                }
            }
            IrStmt::SetChildError(e) => self.expr(e),
            IrStmt::Require(_) => {}
            IrStmt::RawText(s) => {
                // raw code blob: `$` refs inside execute
                for v in dollar_vars(s) {
                    self.mention(&v);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                self.expr(discriminant);
                for c in clauses {
                    for b in &c.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                self.with_risky(|w| {
                    for b in inner {
                        w.stmt(b);
                    }
                    for r in redirects {
                        w.expr(&r.target);
                    }
                });
            }
            IrStmt::Function { name, body, named_blocks } => {
                // nested function: its own scope (a use inside is NOT a
                // use of the outer scope, even for same-named bindings —
                // merging would only ever widen toward global, sound)
                self.with_scope(Scope::Func(name.clone()), |w| {
                    for b in body {
                        w.top_stmt(b);
                    }
                    for (_, b) in named_blocks {
                        for st in b {
                            w.top_stmt(st);
                        }
                    }
                });
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                self.with_risky(|w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrStmt::Block(body) => {
                for b in body {
                    self.stmt(b);
                }
            }
            IrStmt::Select { clauses } => {
                for c in clauses {
                    if let Some(t) = &c.target {
                        // recv target is a binding — skip
                        let _ = t;
                    }
                    if let Some(ch) = &c.ch {
                        self.expr(ch);
                    }
                    if let Some(v) = &c.value {
                        self.expr(v);
                    }
                    for b in &c.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::Asm { outputs, inputs, .. } => {
                // inline asm operands: opaque codegen — risky mentions
                // of nothing attributable (constraint strings are not
                // var reads); still walk the operand exprs
                self.with_risky(|w| {
                    for (_, e) in outputs.iter().chain(inputs) {
                        w.expr(e);
                    }
                });
            }
            IrStmt::Expr(e) => self.expr(e),
            IrStmt::Label(_) | IrStmt::Goto(_) => {}
            IrStmt::Shaderize { body, .. } => {
                for b in body {
                    self.stmt(b);
                }
            }
        }
    }

    fn expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Range { .. } => {}
            IrExpr::Str(s, style) => {
                // QUOTED text is DATA: `a = "a"` must not mention `a` (a
                // phantom read before the write defeats the pass-2 local
                // rule). An UNQUOTED word is a reference — the frontends
                // hand some operands over already substituted (`"$x"` can
                // arrive as a bare `x`).
                let quoted = !matches!(style, crate::ir::StrStyle::Raw);
                self.mentions_value_str_style(s, quoted);
            }
            IrExpr::Var(v, _) | IrExpr::Ident(v) => self.mention(v),
            IrExpr::Index { var, key } => {
                self.mention(var);
                self.expr(key);
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            IrExpr::Call { func, args } => {
                // `local x=...` / `declare x=...` (and friends): the
                // `name=` head word BINDS the name — route through the
                // decl walk so the write is attributed to the current
                // scope (see `walk_decl_args`).
                if matches!(func.as_str(), "exec" | "fnCall" | "callDirect" | "builtin") {
                    if let Some((IrExpr::Str(h, _), rest)) = args.split_first() {
                        if matches!(h.as_str(), "local" | "declare" | "typeset" | "export" | "readonly") {
                            self.walk_decl_args(rest);
                            return;
                        }
                    }
                    // callee-name word skipped (a literal, not a read);
                    // dynamic callees walk fully risky
                    match args.split_first() {
                        Some((IrExpr::Str(h, _), rest)) => {
                            self.dispatch_args(Some(h.as_str()), rest);
                        }
                        _ => {
                            for a in args {
                                self.with_risky(|w| w.expr(a));
                            }
                        }
                    }
                    return;
                }
                // Direct user-function call: inline dispatch.
                if self.defined.contains(func) {
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
                // Inline value calls render where they stand (safe);
                // everything else may lower through a file-scope helper
                // embedding arg reads — risky. (Drift risk if the
                // renderer adds helper paths: the C backend's
                // post-render guard fails loud, never silent.)
                if is_inline_call(func) {
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
                self.with_risky(|w| {
                    for a in args {
                        w.expr(a);
                    }
                });
            }
            IrExpr::MethodCall { obj, args, .. } => {
                self.expr(obj);
                for a in args {
                    self.expr(a);
                }
            }
            IrExpr::Ternary { cond, then, else_ } => {
                self.expr(cond);
                self.expr(then);
                self.expr(else_);
            }
            IrExpr::DefinedOr { expr, default } => {
                self.expr(expr);
                self.expr(default);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => self.mentions_str(s),
                        InterpPart::Expr(x) => self.expr(x),
                    }
                }
            }
            IrExpr::Capture { expr, .. } => {
                self.with_risky(|w| w.expr(expr));
            }
            IrExpr::Regex { .. } => {}
            IrExpr::RawExpr(s) => {
                // raw code blob: `$` refs inside execute
                for v in dollar_vars(s) {
                    self.mention(&v);
                }
            }
            IrExpr::Arrow(body) => {
                self.with_risky(|w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrExpr::Array(items) => {
                for it in items {
                    self.expr(it);
                }
            }
            IrExpr::ArrayComp { iter, elem, cond, .. } => {
                // comp var is a binding — skip; the comprehension
                // desugars to a loop (inline), but treat conservatively
                self.with_risky(|w| {
                    w.expr(iter);
                    w.expr(elem);
                    if let Some(c) = cond {
                        w.expr(c);
                    }
                });
            }
            IrExpr::Lambda { body, .. } => {
                // params are bindings — skip; body is a closure
                self.with_risky(|w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Json(_) => {}
            IrExpr::Object(props) => {
                for (_, v) in props {
                    self.expr(v);
                }
            }
            IrExpr::Splice(x) => self.expr(x),
            IrExpr::Ext(n) => {
                for c in n.children() {
                    self.expr(c);
                }
            }
        }
    }

    fn arith(&mut self, a: &ArithAst) {
        match a {
            ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
            ArithAst::Var(v) | ArithAst::Ident(v) => self.mention(v),
            ArithAst::Index { var, key } => {
                self.mention(var);
                self.arith(key);
            }
            ArithAst::Bin { lhs, rhs, .. } => {
                self.arith(lhs);
                self.arith(rhs);
            }
            ArithAst::Un { arg, .. } => self.arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                self.arith(test);
                self.arith(then);
                self.arith(else_);
            }
            ArithAst::Assign { var, rhs, .. } => {
                self.mention(var);
                self.arith(rhs);
            }
            ArithAst::IncDec { var, .. } => self.mention(var),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// Shared call-argument dispatch (Exec statements and exec/fnCall
    /// Call expressions): defined callees render inline (args safe);
    /// echo/printf render natively arg-by-arg exactly when the arg
    /// satisfies `echo_mirror_ok`; every other callee may lower through
    /// a file-scope helper embedding arg reads (t80), so those args are
    /// risky. Drift risk if the renderer adds helper paths: the C
    /// backend's post-render guard fails loud, never silent.
    /// `local`/`declare`/`export`-family words: `name=` heads are
    /// top-level WRITE mentions in the current scope; flag words are
    /// skipped; every other word (values, nested calls) walks normally.
    fn walk_decl_args(&mut self, args: &[IrExpr]) {
        // flatten one Array wrapper (the exec argv shape)
        let flat: Vec<&IrExpr> = args
            .iter()
            .flat_map(|a| match a {
                IrExpr::Array(items) => items.iter().collect(),
                other => vec![other],
            })
            .collect();
        for w in flat {
            match w {
                IrExpr::Str(t, _) => {
                    if t.starts_with('-') {
                        continue;
                    }
                    if let Some((h, _)) = t.split_once('=') {
                        if is_ident_word(h) {
                            self.mention_write(h);
                        }
                    }
                }
                IrExpr::Array(items) => {
                    for it in items {
                        self.expr(it);
                    }
                }
                _ => {
                    self.expr(w);
                }
            }
        }
    }

    fn dispatch_args(&mut self, head: Option<&str>, args: &[IrExpr]) {
        // flatten one Array wrapper (the exec argv shape
        // `exec(cmd, [words])`) so the echo mirror applies per word,
        // not to the wrapper itself
        let flat: Vec<&IrExpr> = args
            .iter()
            .flat_map(|a| match a {
                IrExpr::Array(items) => items.iter().collect(),
                other => vec![other],
            })
            .collect();
        let args = flat;
        match head {
            Some(f) if self.defined.contains(f) => {
                for a in args {
                    self.expr(a);
                }
            }
            Some(f) if f == "echo" || f == "printf" => {
                for a in args {
                    if echo_mirror_ok(a) {
                        self.expr(a);
                    } else {
                        self.with_risky(|w| w.expr(a));
                    }
                }
            }
            _ => {
                for a in args {
                    self.with_risky(|w| w.expr(a));
                }
            }
        }
    }
}

/// Uniform string rule: exact identifier match (a bare name
/// argument), a `name=value` head (a builtin write word), and `$`
/// references. All conservative — over-mentioning only ever widens
/// toward global, never miscompiles.
fn str_mention_all(text: &str, out: &mut impl FnMut(&str)) {
    if is_ident_word(text) {
        out(text);
    } else if let Some((head, _)) = text.split_once('=') {
        if is_ident_word(head) {
            out(head);
        }
    }
    for v in dollar_vars(text) {
        out(&v);
    }
}

/// Every identifier token in a code-ish word (`let i++`, eval args).
pub(crate) fn all_idents(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let mut out = vec![];
    while i < b.len() {
        if (b[i].is_ascii_alphabetic() || b[i] == b'_')
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
        {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(text[start..i].to_string());
        } else {
            i += 1;
        }
    }
    out
}

/// Mirrors the renderer's `echo_native_ok` (Str/Int/Var/Ident/Arith/
/// BinOp/Bool safe; calls safe except split/captureWords/pipeline —
/// plus capture, which the renderer accepts for ordering but whose
/// cap-site body may embed C idents, so risky here; Interpolate
/// recursed with the same rule; everything else risky).
fn echo_mirror_ok(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(_, _)
        | IrExpr::Int(_)
        | IrExpr::Var(_, _)
        | IrExpr::Ident(_)
        | IrExpr::Arith(_)
        | IrExpr::BinOp { .. }
        | IrExpr::Bool(_) => true,
        IrExpr::Call { func, .. } => {
            !matches!(func.as_str(), "split" | "captureWords" | "pipeline" | "capture")
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => echo_mirror_ok(x),
        }),
        _ => false,
    }
}

/// Call forms that always render inline (never in a file-scope
/// helper): the value-call vocabulary. Everything else (exec/fnCall,
/// user-function dispatch, test, unknown) is assumed helper-capable
/// and walked risky. Keep in sync with the C renderer's fallback
/// paths; the post-render guard catches drift loudly.
fn is_inline_call(func: &str) -> bool {
    matches!(
        func,
        // Pure test/truth forms: their ARGUMENTS are the only reads, and
        // every backend renders them INLINE (a `!x.is_empty()` / `.len == 0`
        // / `s2s(x) == ""` expression) rather than through a file-scope
        // helper that would capture the argument. Without this a read inside
        // one is marked RISKY, and `compute_local_vars` refuses the name — so
        // the py frontend's `if not x:` (`test("-z \"$x\"")`) kept `x` a
        // thread_local even once the borrow verdict named it (t27).
        "test" | "testArith" | "not" | "truthy" | "isTruthy"
            | "getVar" | "setVar" | "assign" | "setArray" | "setArrayAppend"
            | "arrayIndex" | "arrayLen" | "arrayItems" | "sortedIntJoin"
            | "sortedBigintJoin" | "max" | "min" | "sum" | "param"
            | "arith" | "arithEval" | "join" | "split" | "listVar"
    )
}

fn is_ident_word(w: &str) -> bool {
    !w.is_empty()
        && w.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !w.bytes().next().is_some_and(|b| b.is_ascii_digit())
}

/// `$`-reference variable names in a text.
pub(crate) fn dollar_vars(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let mut out = vec![];
    while i < b.len() {
        if b[i] != b'$' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= b.len() {
            break;
        }
        if b[i] == b'{' {
            i += 1;
            if i < b.len() && (b[i] == b'#' || b[i] == b'!') {
                i += 1;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if start < i {
                out.push(text[start..i].to_string());
            }
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(text[start..i].to_string());
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, StrStyle};

    fn prog(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
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

    fn word(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    #[test]
    fn single_scope_uses_are_safe() {
        // setArray + append + index read, all top-level in one block
        let p = prog(vec![
            IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "xs".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Call {
                    func: "setArray".to_string(),
                    args: vec![word("xs"), IrExpr::Array(vec![IrExpr::Int(1)])],
                },
                asm: None,
            },
            IrStmt::While {
                cond: IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "<".to_string(),
                    lhs: Box::new(ArithAst::Var("i".to_string())),
                    rhs: Box::new(ArithAst::Num(3)),
                })),
                body: vec![IrStmt::Expr(IrExpr::Call {
                    func: "arrayIndex".to_string(),
                    args: vec![word("xs"), word("0")],
                })],
            },
        ]);
        let u = use_scopes(&p);
        let xs = &u["xs"];
        assert!(xs.iter().all(|x| x.scope == Scope::Main && !x.risky));
    }

    #[test]
    fn capture_use_is_risky() {
        let p = prog(vec![IrStmt::Expr(IrExpr::Capture {
            expr: Box::new(IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![word("xs")],
            }),
            native: false,
        })]);
        let u = use_scopes(&p);
        assert!(u["xs"].iter().any(|x| x.risky));
    }

    #[test]
    fn nested_function_is_own_scope() {
        let inner = IrStmt::Function {
            name: "g".to_string(),
            body: vec![IrStmt::Expr(IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![word("xs")],
            })],
            named_blocks: vec![],
        };
        let p = prog(vec![
            IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "xs".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Call {
                    func: "setArray".to_string(),
                    args: vec![word("xs"), IrExpr::Array(vec![])],
                },
                asm: None,
            },
            IrStmt::Function {
                name: "f".to_string(),
                body: vec![inner],
                named_blocks: vec![],
            },
        ]);
        let u = use_scopes(&p);
        let scopes: HashSet<Scope> =
            u["xs"].iter().map(|x| x.scope.clone()).collect();
        // Main (def) + g (read): not single-scope
        assert!(scopes.contains(&Scope::Main));
        assert!(scopes.contains(&Scope::Func("g".to_string())));
    }
}

/// Sole owning function for function-local storage: `Some(F)` when every
/// mention sits in function scope `F` with no risky positions, else
/// `None` (global storage). Top-level-only (`Main`) and multi-scope uses
/// stay global. Recursion-safe by construction: locals are per-activation.
pub fn local_owner(uses: &[Use]) -> Option<String> {
    use std::collections::BTreeSet;
    let mut scopes: BTreeSet<Scope> = BTreeSet::new();
    for u in uses {
        if u.risky {
            return None;
        }
        scopes.insert(u.scope.clone());
    }
    if scopes.len() == 1 {
        if let Some(Scope::Func(f)) = scopes.iter().next() {
            return Some(f.clone());
        }
    }
    None
}

/// Pass-2 variant of [`local_owner`]: the single non-risky scope that
/// owns a name, **including** top-level `Main` (whose owner is Rust's
/// `fn main`). Anything multi-scope or risky yields `None` (global
/// storage stays the only sound choice).
pub fn local_scope(uses: &[Use]) -> Option<Scope> {
    use std::collections::BTreeSet;
    let mut scopes: BTreeSet<Scope> = BTreeSet::new();
    for u in uses {
        if u.risky {
            return None;
        }
        scopes.insert(u.scope.clone());
    }
    if scopes.len() == 1 {
        return scopes.into_iter().next();
    }
    None
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    #[test]
    fn single_safe_function_scope_has_owner() {
        let uses = vec![
            Use { scope: Scope::Func("f".to_string()), risky: false, write: false, top: true },
            Use { scope: Scope::Func("f".to_string()), risky: false, write: false, top: true },
        ];
        assert_eq!(local_owner(&uses), Some("f".to_string()));
    }

    #[test]
    fn risky_use_forces_global() {
        let uses = vec![
            Use { scope: Scope::Func("f".to_string()), risky: false, write: false, top: true },
            Use { scope: Scope::Func("f".to_string()), risky: true, write: false, top: true },
        ];
        assert_eq!(local_owner(&uses), None);
    }

    #[test]
    fn local_scope_includes_main() {
        let uses = vec![
            Use { scope: Scope::Main, risky: false, write: false, top: true },
            Use { scope: Scope::Main, risky: false, write: false, top: true },
        ];
        assert_eq!(local_scope(&uses), Some(Scope::Main));
        assert_eq!(local_owner(&uses), None);
    }

    #[test]
    fn main_scope_stays_global() {
        let uses = vec![Use { scope: Scope::Main, risky: false, write: false, top: true }];
        assert_eq!(local_owner(&uses), None);
    }
}

#[cfg(test)]
mod exec_tests {
    use super::*;
    use crate::ir::{IrProgram, StrStyle};

    fn word(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn prog(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
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

    #[test]
    fn exec_index_word_is_risky() {
        // t80 regression: `print(x[0])` lowers to exec-echo whose arg
        // is an `Index` node — `echo_native_ok` refuses it (wildcard),
        // so the echo shell-fallbacks into a site helper embedding the
        // C ident (`x[0]` / `x_len` in `_sh_site_1`). The use must force
        // global. (An `arrayIndex` *call* arg would stay safe — the
        // renderer treats that shape natively. The mirror is exact.)
        let p = prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                word("echo"),
                IrExpr::Array(vec![IrExpr::Index {
                    var: "xs".to_string(),
                    key: Box::new(word("0")),
                }]),
            ],
        })]);
        let u = use_scopes(&p);
        assert!(u["xs"].iter().any(|x| x.risky));
        assert_eq!(local_owner(&u["xs"]), None);
    }

    #[test]
    fn exec_native_call_arg_stays_safe() {
        // `arrayIndex`/`sortedIntJoin`-shaped echo args render natively
        // inline (not in split/captureWords/pipeline) — safe.
        let p = prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                word("echo"),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "arrayIndex".to_string(),
                    args: vec![word("xs"), word("0")],
                }]),
            ],
        })]);
        let u = use_scopes(&p);
        assert!(u["xs"].iter().all(|x| !x.risky));
    }

    #[test]
    fn plain_value_call_stays_safe() {
        // sortedIntJoin/max/arrayIndex reads render inline — safe.
        let p = prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "sortedIntJoin".to_string(),
            args: vec![word("xs")],
        })]);
        let u = use_scopes(&p);
        assert!(u["xs"].iter().all(|x| !x.risky));
    }
}

#[cfg(test)]
mod local_decl_tests {
    use super::*;
    use crate::ir::{IrProgram, StrStyle};

    fn prog(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
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

    fn word(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    /// `local x=$1` inside `helper` + a `getVar(x)` read: x must be a
    /// top-level WRITE in Func(helper) scope (the shape pass-2 locals
    /// need to fire).
    #[test]
    fn local_decl_is_a_write_in_fn_scope() {
        let p = prog(vec![IrStmt::Function {
            name: "helper".to_string(),
            body: vec![
                IrStmt::Expr(IrExpr::Call {
                    func: "builtin".to_string(),
                    args: vec![
                        word("local"),
                        IrExpr::Array(vec![
                            word("x="),
                            IrExpr::Call {
                                func: "getVar".to_string(),
                                args: vec![word("1")],
                            },
                        ]),
                    ],
                }),
                IrStmt::Expr(IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![word("x")],
                }),
            ],
            named_blocks: vec![],
        }]);
        let u = use_scopes(&p);
        let xs = &u["x"];
        assert!(
            xs.iter().any(|x| x.write && x.top && x.scope == Scope::Func("helper".to_string())),
            "no top-level write in fn scope: {xs:?}"
        );
        assert!(xs.iter().any(|x| !x.write), "no read: {xs:?}");
        assert!(xs.iter().all(|x| !x.risky), "risky: {xs:?}");
        assert_eq!(local_scope(xs), Some(Scope::Func("helper".to_string())));
    }
}
