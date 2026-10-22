//! numeric-observed — shared shIR analysis for int-homing of string tables.
//!
//! Answers a language-neutral question: *which scalars, function-parameter
//! slots, and array elements are only ever observed as numbers?* A cell is
//! "numeric-observed" when every **read** of it reaches the program's output
//! exclusively through a numeric coercion — an `$(( ))` operand, an array
//! subscript key, a `-lt`/`-eq` test, a copy into another numeric-observed
//! cell, or a call argument whose callee param slot is itself numeric-observed
//! — and never through a text observer (`echo`, interpolation, a `=`/`-z`
//! string test, a redirect target, or a whole-array read like a join/slice/
//! iteration).
//!
//! # Why this exists
//!
//! The C backend homes a shell array to `long long[]` only when every stored
//! value is *provably a canonical integer* (`int_element_verdict`'s `val_int`
//! rule). That misses a large, common class: **string tables whose elements are
//! written from non-canonical text but read only numerically.** The paradigm is
//! a packed-string texture/lookup table that a loader splits with substring
//! ops (`${p%% *}`), clamps, and stores as text — every consumer then `atoll`s
//! the element. The stored text is unproven-canonical (a slice `"12a"` atolls to
//! 12 but is not int text), so the canonical-value rule refuses; yet because
//! every reader re-parses, storing the *parsed value* is observationally
//! identical. Homing such an array to `long long[]` deletes the per-read
//! `strdup`/`atoll` round trip — on a per-pixel render loop that is the whole
//! game.
//!
//! # What this analysis proves
//!
//! `analyze(prog).array(name)` is true when all element reads of `name` are
//! numeric-observed. An element read `A[k]` is numeric-observed when its value
//! flows only into numeric contexts — including into a scalar that is itself
//! numeric-observed, or into a function-param slot that is itself
//! numeric-observed (interprocedural). This mechanizes the hand-traced chain in
//! the MIMEcroft renderer: `rbx_r = RBTEX_R[i]` → `atoll(rbx_r)` (arith), →
//! `rb_mkhex(rbx_r,…)` whose `$1` is copied to `rbm_r` and only ever used as a
//! `RBHEX[atoll(rbm_r)]` subscript key, → `rbov_fr = rbx_r` which is itself
//! only `atoll`-read. All numeric; none observes the text.
//!
//! # Soundness (greatest fixpoint, removal-only)
//!
//! Start optimistic — every candidate scalar, param slot, and array is
//! numeric-observed — then **remove** any cell with an inherently-text use or a
//! use that depends on a cell not (or no longer) in the set. Iterate to a fixed
//! point. Removal only shrinks each set, so it terminates and yields the
//! *largest* sound assignment. Any use we cannot positively recognize as
//! numeric is treated as text (removing the cell) — the walk is conservative by
//! default. Backends further re-validate at their own ingest (the C backend
//! still applies its dense-cover, OOB, and bigint gates before homing), so this
//! verdict is a necessary enabling fact, never a sufficient one alone.
//!
//! # Sharing
//!
//! The *fact* — "these elements are only ever read as numbers" — holds for every
//! backend: a Perl/Python/JS renderer can equally keep such a table in a native
//! numeric array instead of re-parsing strings per read. This module therefore
//! lives in the shared layer; the *storage decision* (`char**` → `long long[]`
//! is sound because C's `atoll` is idempotent on the value) stays backend-owned.

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, StrStyle};
use std::collections::{HashMap, HashSet};

/// How the value of a read is consumed. This is the whole classification:
/// a cell stays numeric-observed iff every one of its reads has a "served"
/// consumption under the current fixpoint membership.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Use {
    /// Inherently numeric context (`$(( ))` operand, subscript key, `-lt`).
    Num,
    /// Text observer (echo, interpolation, `=`/`-z` test, redirect, unknown).
    Text,
    /// Flows into a scalar cell (a copy `t = v`).
    ToCell(String),
    /// Flows into a callee's positional param slot (`f(v)` → `$i` of `f`).
    ToParam(String, usize),
    /// Flows into a callee's WHOLE positional stream: an unquoted `f $x`
    /// argument spreads `split()` words over `$1`, `$2`, … — served only
    /// when EVERY param slot the callee reads is numeric-observed.
    ToParamSpread(String),
    /// Flows into an array element (`A[k] = v`, `A+=(v)`), later read by `A`.
    ToArray(String),
}

/// Result of the analysis: numeric-observed scalars, param slots, arrays —
/// plus the collected use-maps, so a backend can walk the downstream
/// value-flow chain of an array ([`NumericObserved::array_chain`]).
#[derive(Default)]
pub struct NumericObserved {
    scalars: HashSet<String>,
    params: HashSet<(String, usize)>,
    arrays: HashSet<String>,
    scalar_uses: HashMap<String, Vec<Use>>,
    param_uses: HashMap<(String, usize), Vec<Use>>,
    array_uses: HashMap<String, Vec<Use>>,
    /// Every param slot a function's body reads (pre-fixpoint candidates).
    fn_slots: HashMap<String, Vec<usize>>,
}

impl NumericObserved {
    pub fn scalar(&self, name: &str) -> bool {
        self.scalars.contains(name)
    }
    pub fn param(&self, func: &str, idx: usize) -> bool {
        self.params.contains(&(func.to_string(), idx))
    }
    /// Enabling fact: every element read of `name` reaches only numeric
    /// contexts. A backend combines this with its own storage soundness gates
    /// before homing the array to a native integer array.
    pub fn array(&self, name: &str) -> bool {
        self.arrays.contains(name)
    }

    /// Downstream value-flow closure of reads of `name`: every scalar that
    /// a value stored in `name` can reach transitively through copies,
    /// param slots (incl. split-spreads), and array writes. A backend can
    /// veto homing when any chain member parses its store text with wider
    /// semantics than `atoll` (bigint exact parse, float parse, UInt64).
    pub fn array_chain(&self, name: &str) -> HashSet<String> {
        let mut out: HashSet<String> = HashSet::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: Vec<Use> = self.array_uses.get(name).cloned().unwrap_or_default();
        while let Some(u) = queue.pop() {
            match u {
                Use::Num | Use::Text => {}
                Use::ToCell(v) => {
                    if seen.insert(format!("s:{v}")) {
                        out.insert(v.clone());
                        if let Some(us) = self.scalar_uses.get(&v) {
                            queue.extend(us.iter().cloned());
                        }
                    }
                }
                Use::ToParam(f, i) => {
                    let k = format!("p:{f}:{i}");
                    if seen.insert(k) {
                        if let Some(us) = self.param_uses.get(&(f, i)) {
                            queue.extend(us.iter().cloned());
                        }
                    }
                }
                Use::ToParamSpread(f) => {
                    if let Some(slots) = self.fn_slots.get(&f) {
                        for &i in slots {
                            let k = format!("p:{f}:{i}");
                            if seen.insert(k) {
                                if let Some(us) = self.param_uses.get(&(f.clone(), i)) {
                                    queue.extend(us.iter().cloned());
                                }
                            }
                        }
                    }
                }
                Use::ToArray(a) => {
                    if seen.insert(format!("a:{a}")) {
                        if let Some(us) = self.array_uses.get(&a) {
                            queue.extend(us.iter().cloned());
                        }
                    }
                }
            }
        }
        out
    }
}

/// Positional-name → 0-based param-slot index (`$1` → slot 0, matching
/// call-site argument position j). `$0` (program name) excluded.
fn positional_idx(name: &str) -> Option<usize> {
    let n = name.strip_prefix('$').unwrap_or(name);
    if n.is_empty() || n == "0" || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse::<usize>().ok().and_then(|k| k.checked_sub(1))
}

/// Test text is numeric-only when it uses only integer comparison ops and no
/// string/file operator. Mirrors the C backend's `test_refs` heuristic.
fn test_is_numeric(text: &str) -> bool {
    const STR_OPS: [&str; 16] = [
        " -z ", "-n ", " = ", " != ", "=~", "-f ", "-d ", "-e ", "-s ", "-r ", "-w ", "-x ", "-L ",
        "-h ", "-v ", "-R ",
    ];
    let has_str = STR_OPS.iter().any(|op| text.contains(op));
    let has_num = ["-lt", "-le ", "-gt", "-ge ", "-eq", "-ne "]
        .iter()
        .any(|op| text.contains(op));
    !has_str && has_num
}

/// Structural collector: walks the program once, recording each read's
/// consumption. `enc` is the enclosing function name (for positional slots).
struct Collector {
    defined_fns: HashSet<String>,
    scalars: HashSet<String>,                        // candidate set
    params: HashSet<(String, usize)>,                // candidate set
    arrays: HashSet<String>,                         // candidate set
    scalar_uses: HashMap<String, Vec<Use>>,
    param_uses: HashMap<(String, usize), Vec<Use>>,
    array_uses: HashMap<String, Vec<Use>>,
    /// Functions whose body reads `$@`/`$*` — every param slot of those is
    /// text-observed (applied as a post-pass: word count is dynamic).
    rest_positionals: HashSet<String>,
}

impl Collector {
    /// Record a read of a plain name consumed by `sink`.
    fn read_name(&mut self, name: &str, sink: &Use, enc: Option<&str>) {
        if let Some(i) = positional_idx(name) {
            if let Some(f) = enc {
                self.params.insert((f.to_string(), i));
                self.param_uses.entry((f.to_string(), i)).or_default().push(sink.clone());
            }
            // top-level `$1` (program arg): not a tracked cell; skip.
            return;
        }
        self.scalars.insert(name.to_string());
        self.scalar_uses.entry(name.to_string()).or_default().push(sink.clone());
    }

    fn read_array(&mut self, name: &str, sink: &Use) {
        self.arrays.insert(name.to_string());
        self.array_uses.entry(name.to_string()).or_default().push(sink.clone());
    }

    /// Whole-positionals (`$@`/`$*`) read inside `enc` → post-pass marks
    /// every param slot of that function text-observed.
    fn mark_rest_positionals(&mut self, enc: Option<&str>) {
        if let Some(f) = enc {
            self.rest_positionals.insert(f.to_string());
        }
    }

    /// Scan raw text for `$name` / `${name…}` expansions and record each read
    /// with `sink`. This is what makes `Str("$x")` args visible — the parser
    /// leaves raw `$ref`s in DoubleQuoted strings (arrayIndex keys, printf
    /// formats, command args). SingleQuoted/Raw never expand → skipped.
    fn record_text_expansions(&mut self, text: &str, sink: &Use, enc: Option<&str>) {
        let b = text.as_bytes();
        let mut i = 0usize;
        while i < b.len() {
            if b[i] != b'$' || (i > 0 && b[i - 1] == b'\\') {
                i += 1;
                continue;
            }
            i += 1;
            if i < b.len() && b[i] == b'{' {
                i += 1;
                let st = i;
                while i < b.len() && b[i] != b'}' {
                    i += 1;
                }
                let inner = &text[st..i.min(b.len())];
                if i < b.len() {
                    i += 1; // consume '}'
                }
                self.record_expansion(inner, sink, enc);
            } else if i < b.len() && (b[i] == b'@' || b[i] == b'*') {
                i += 1;
                self.mark_rest_positionals(enc);
            } else {
                let st = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if st == i {
                    continue; // `$?`, `$$`, trailing `$` — no cell
                }
                self.record_expansion(&text[st..i], sink, enc);
            }
        }
    }

    /// One expansion's text: `name`, `name[sub]`, `#len` forms.
    fn record_expansion(&mut self, inner: &str, sink: &Use, enc: Option<&str>) {
        let inner = inner.trim_start_matches('$'); // `${$x}` edge
        if inner == "@" || inner == "*" {
            self.mark_rest_positionals(enc);
            return;
        }
        let (is_len, body) = match inner.strip_prefix('#') {
            Some(rest) => (true, rest),
            None => (false, inner),
        };
        if body.is_empty() {
            return; // `${#}` = argc; observes no cell value
        }
        self.record_name_form(body, sink, enc, is_len);
    }

    /// Record a read of a name in expansion position: `x`, `A[$i]`, `A[@]`,
    /// possibly with a trailing modifier (`:-d`, `:0:3`, `%% p`, `@Q`).
    ///
    /// - plain `x` may also denote an array (`${A}` = `A[0]`) → dual-record
    ///   both namespaces with the same sink. A spurious entry for the wrong
    ///   namespace can only cause a conservative *drop* — backends query
    ///   `array(name)` only for names they themselves saw element writes on.
    /// - `[@]`/`[*]` = whole-array join → always Text (under `len` it is a
    ///   count and observes nothing).
    /// - `[k]` = element read → sink; key refs are numeric.
    /// - a non-value-preserving modifier (`:0:3`, `%% p`, `@Q`, `!`-indirect)
    ///   observes the *text*; a value-preserving one (`:-` `:=` `:+` `:?`)
    ///   lets the value flow through to `sink`.
    fn record_name_form(&mut self, text: &str, sink: &Use, enc: Option<&str>, is_len: bool) {
        let text = text.trim();
        let (base, key) = match text.find('[') {
            Some(p) => (
                &text[..p],
                Some(text[p + 1..].trim_end_matches(']').trim().to_string()),
            ),
            None => (text, None),
        };
        // head identifier; anything after it is a modifier.
        let ident_end = base
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(base.len());
        let ident = &base[..ident_end];
        let rest = &base[ident_end..];
        if ident.is_empty() {
            return;
        }
        let value_preserving = rest.is_empty()
            || rest.starts_with(":-")
            || rest.starts_with(":=")
            || rest.starts_with(":+")
            || rest.starts_with(":?")
            || rest.starts_with('=')
            || rest.starts_with('+')
            || rest.starts_with('?');
        let sink2 = if is_len || !value_preserving {
            Use::Text
        } else {
            sink.clone()
        };
        match key {
            Some(ref k) if k == "@" || k == "*" => {
                if !is_len {
                    self.read_array(ident, &Use::Text); // whole-array join
                }
                // `${#A[@]}` = count → observes nothing
            }
            Some(ref k) => {
                self.read_array(ident, &sink2);
                for r in dollar_refs(k) {
                    self.read_name(&r, &Use::Num, enc);
                }
            }
            None => {
                self.read_name(ident, &sink2, enc);
                if positional_idx(ident).is_none() {
                    self.read_array(ident, &sink2);
                }
            }
        }
        // refs inside a modifier (`${x:-$d}`, `${x:$i:3}`): value-preserving
        // modifiers pass the sink through, transforms observe as text.
        if !rest.is_empty() {
            for r in dollar_refs(rest) {
                self.read_name(&r, &sink2, enc);
            }
        }
    }

    fn walk_stmts(&mut self, stmts: &[IrStmt], enc: Option<&str>) {
        for s in stmts {
            self.walk_stmt(s, enc);
        }
    }

    fn walk_stmt(&mut self, s: &IrStmt, enc: Option<&str>) {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    let idxs = &t.indices;
                    if !idxs.is_empty() {
                        // `A[k] = expr`: key(s) numeric, value flows into A.
                        for ix in idxs {
                            self.walk_expr(ix, &Use::Num, enc);
                        }
                        let base = t.var.split('[').next().unwrap_or(&t.var).to_string();
                        self.walk_expr(expr, &Use::ToArray(base), enc);
                    } else if let Some((base, key)) = t.var.split_once('[') {
                        // baked subscript `A[$k] = expr`
                        self.read_array(base, &Use::ToArray(base.to_string()));
                        self.walk_expr(
                            &IrExpr::Str(key.trim_end_matches(']').to_string(), crate::ir::StrStyle::DoubleQuoted),
                            &Use::Num,
                            enc,
                        );
                        self.walk_expr(expr, &Use::ToArray(base.to_string()), enc);
                    } else {
                        // plain `v = expr`: value flows into scalar v.
                        self.walk_expr(expr, &Use::ToCell(t.var.clone()), enc);
                    }
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                if let Some(e) = init {
                    if let Some(d) = vars.first() {
                        self.walk_expr(e, &Use::ToCell(d.name.clone()), enc);
                    } else {
                        self.walk_expr(e, &Use::Text, enc);
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                for el in elements {
                    self.walk_expr(el, &Use::ToArray(var.clone()), enc);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                self.walk_cond(cond, enc);
                self.walk_stmts(then, enc);
                for (c, b) in elsifs {
                    self.walk_cond(c, enc);
                    self.walk_stmts(b, enc);
                }
                self.walk_stmts(else_, enc);
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                self.walk_cond(cond, enc);
                self.walk_stmts(body, enc);
            }
            IrStmt::For { iter, body, .. } => {
                // iteration observes elements as text unless it's an explicit
                // numeric index loop; conservatively mark whole-array Text.
                self.mark_iter_arrays(iter, &Use::Text, enc);
                self.walk_expr(iter, &Use::Text, enc);
                self.walk_stmts(body, enc);
            }
            IrStmt::ForInit { init, cond, step, body, .. } => {
                self.walk_stmts(init, enc);
                self.walk_cond(cond, enc);
                self.walk_stmts(step, enc);
                self.walk_stmts(body, enc);
            }
            IrStmt::Function { name, body, named_blocks, .. } => {
                self.walk_stmts(body, Some(name));
                for (_, b) in named_blocks {
                    self.walk_stmts(b, Some(name));
                }
            }
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                self.walk_stmts(b, enc)
            }
            IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
                self.walk_stmts(body, enc);
                for e in excepts {
                    self.walk_stmts(&e.body, enc);
                }
                self.walk_stmts(else_body, enc);
                self.walk_stmts(finally_body, enc);
            }
            IrStmt::Case { discriminant, clauses } => {
                self.walk_expr(discriminant, &Use::Text, enc);
                for c in clauses {
                    self.walk_stmts(&c.body, enc);
                }
            }
            IrStmt::Select { clauses } => {
                for c in clauses {
                    self.walk_stmts(&c.body, enc);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                self.walk_stmts(inner, enc);
                for r in redirects {
                    self.walk_expr(&r.target, &Use::Text, enc);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    self.walk_stmts(st, enc);
                }
            }
            IrStmt::Output { value, .. } => self.walk_expr(value, &Use::Text, enc),
            IrStmt::Expr(e) => self.walk_expr(e, &Use::Text, enc),
            _ => {}
        }
    }

    /// Condition: arith → numeric; `test` → numeric-op detection; else text.
    fn walk_cond(&mut self, cond: &IrExpr, enc: Option<&str>) {
        if let IrExpr::Arith(a) = cond {
            self.walk_arith(a, enc);
            return;
        }
        if let IrExpr::Call { func, args } = cond {
            if func == "test" {
                if let Some(IrExpr::Str(t, _)) = args.first() {
                    let numeric = test_is_numeric(t);
                    let sink = if numeric { Use::Num } else { Use::Text };
                    self.mark_test_refs(t, &sink, enc);
                    return;
                }
            }
        }
        self.walk_expr(cond, &Use::Text, enc);
    }

    /// Record every `$…` reference inside a `test` text with `sink`.
    fn mark_test_refs(&mut self, text: &str, sink: &Use, enc: Option<&str>) {
        self.record_text_expansions(text, sink, enc);
    }

    /// Whole-array references in a `for … in "${A[@]}"` iterator → Text.
    fn mark_iter_arrays(&mut self, iter: &IrExpr, sink: &Use, enc: Option<&str>) {
        if let IrExpr::Array(items) = iter {
            for it in items {
                if let IrExpr::Call { func, args } = it {
                    if matches!(func.as_str(), "arrayItems" | "param") {
                        for a in args {
                            if let IrExpr::Str(nm, _) = a {
                                self.record_name_form(nm, sink, enc, false);
                            }
                        }
                    }
                }
            }
        }
    }

    fn walk_expr(&mut self, e: &IrExpr, sink: &Use, enc: Option<&str>) {
        match e {
            IrExpr::Var(n, _) | IrExpr::Ident(n) => self.read_name(n, sink, enc),
            IrExpr::Str(s, style) => {
                // Raw `$refs` in double-quoted/heredoc text are store reads
                // (`arrayIndex(A, "$i")` keys, `printf "%s" "$x"` args).
                // SingleQuoted/Raw never expand.
                if !matches!(style, StrStyle::SingleQuoted | StrStyle::Raw) {
                    self.record_text_expansions(s, sink, enc);
                }
            }
            IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Json(_) => {}
            IrExpr::Arith(a) => self.walk_arith(a, enc),
            IrExpr::Index { var, key } => {
                // `A[key]` element read flows to sink; key numeric.
                self.read_array(var, sink);
                self.walk_expr(key, &Use::Num, enc);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Expr(x) => self.walk_expr(x, &Use::Text, enc),
                        crate::ir::InterpPart::Lit(s) => {
                            // literal text around the parts: any `$ref` left
                            // in it is expanded → text observation.
                            self.record_text_expansions(s, &Use::Text, enc);
                        }
                    }
                }
            }
            IrExpr::Capture { expr, .. } => self.walk_expr(expr, &Use::Text, enc),
            IrExpr::Array(items) => {
                for it in items {
                    self.walk_expr(it, sink, enc);
                }
            }
            IrExpr::Arrow(stmts) => self.walk_stmts(stmts, enc),
            IrExpr::Call { func, args } => self.walk_call(func, args, sink, enc),
            IrExpr::BinOp { lhs, rhs, .. } => {
                self.walk_expr(lhs, &Use::Text, enc);
                self.walk_expr(rhs, &Use::Text, enc);
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                self.walk_cond(cond, enc);
                self.walk_expr(then, sink, enc);
                self.walk_expr(else_, sink, enc);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                self.walk_expr(obj, &Use::Text, enc);
                for a in args {
                    self.walk_expr(a, &Use::Text, enc);
                }
            }
            _ => self.walk_expr_generic(e, sink, enc),
        }
    }

    /// Fallback for variants not explicitly recognized → conservative Text.
    fn walk_expr_generic(&mut self, e: &IrExpr, _sink: &Use, enc: Option<&str>) {
        match e {
            IrExpr::DefinedOr { expr, default } => {
                self.walk_expr(expr, &Use::Text, enc);
                self.walk_expr(default, &Use::Text, enc);
            }
            IrExpr::Splice(inner) => self.walk_expr(inner, &Use::Text, enc),
            _ => {}
        }
    }

    fn walk_call(&mut self, func: &str, args: &[IrExpr], sink: &Use, enc: Option<&str>) {
        match func {
            "arrayIndex" => {
                // `arrayIndex(A, k, [default…])`: element read flows to sink;
                // first key numeric, later args (defaults) flow to sink.
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if name.contains('[') {
                        self.record_name_form(name, sink, enc, false);
                    } else {
                        self.read_array(name, sink);
                    }
                } else if let Some(a0) = args.first() {
                    self.walk_expr(a0, sink, enc);
                }
                if let Some(k) = args.get(1) {
                    self.walk_expr(k, &Use::Num, enc);
                }
                for a in args.iter().skip(2) {
                    self.walk_expr(a, sink, enc);
                }
            }
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // `${name}`: value flows to sink; a plain name may also
                    // denote an array (`${A}` = `A[0]`) → dual-record.
                    self.record_name_form(name, sink, enc, false);
                }
            }
            "param" => {
                // param(op, name, …): `${name}` / `${#name}` / `${name:-d}` /
                // `${name:off:len}` / `${name%%pat}` / param("slice", …).
                let op = match args.first() {
                    Some(IrExpr::Str(s, _)) => s.as_str(),
                    _ => "",
                };
                let name = match args.get(1) {
                    Some(IrExpr::Str(s, _)) => s.as_str(),
                    _ => "",
                };
                if name.is_empty() {
                    for a in args {
                        self.walk_expr(a, &Use::Text, enc);
                    }
                    return;
                }
                match op {
                    // value-or-default: the value flows through to `sink`.
                    "" | ":-" | ":=" | ":+" | ":?" | "=" | "+" | "?" => {
                        self.record_name_form(name, sink, enc, false);
                        for a in args.iter().skip(2) {
                            self.walk_expr(a, sink, enc); // default expr
                        }
                    }
                    // `${#name}`: length observes text — except
                    // `${#A[@]}` (a count), handled inside.
                    "len" => self.record_name_form(name, sink, enc, true),
                    // substring / pattern ops observe the *text*.
                    _ => {
                        self.record_name_form(name, &Use::Text, enc, false);
                        for a in args.iter().skip(2) {
                            self.walk_expr(a, &Use::Text, enc); // pattern/off
                        }
                    }
                }
            }
            "arrayLen" => { /* length only; no element observation */ }
            "min" | "max" | "sum" => {
                for a in args {
                    self.walk_expr(a, &Use::Num, enc);
                }
            }
            "test" => {
                if let Some(IrExpr::Str(t, _)) = args.first() {
                    let sink2 = if test_is_numeric(t) { Use::Num } else { Use::Text };
                    self.mark_test_refs(t, &sink2, enc);
                }
            }
            "setArray" | "setArrayAppend" => {
                // args[0] = name (write), args[1] = Array of element values.
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        for it in items {
                            self.walk_expr(it, &Use::ToArray(name.clone()), enc);
                        }
                    }
                }
            }
            "slice" | "arrayItems" | "listVar" | "sortedBigintJoin" => {
                // whole-array (or scalar-substring) text readers.
                for a in args {
                    if let IrExpr::Str(nm, _) = a {
                        self.record_name_form(nm, &Use::Text, enc, false);
                    } else {
                        self.walk_expr(a, &Use::Text, enc);
                    }
                }
            }
            "split" | "wordSplit" => {
                // Word-splitting: the value flows to the result words
                // (split itself observes nothing — the C backend's
                // `value_ll` split arm re-reads the inner expression
                // natively for the same reason: atoll skips whitespace and
                // stops at the first non-digit, so atoll(split(x)) ==
                // atoll(x)). As a CALL ARGUMENT, however, bash spreads the
                // words over MULTIPLE positionals (`f $x` → $1=word0,
                // $2=word1, …): gate the value on EVERY slot the callee
                // reads, not just this argument's own index.
                let propagated = match sink {
                    Use::ToParam(f, _) => Use::ToParamSpread(f.clone()),
                    _ => sink.clone(),
                };
                for a in args {
                    self.walk_expr(a, &propagated, enc);
                }
            }
            "exec" | "builtin" => {
                // dispatch(f, args…): a call to a PROGRAM-DEFINED function
                // (the parser's exec form) — arg j flows into f's positional
                // slot j.
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.defined_fns.contains(name.as_str()) {
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            for (j, it) in items.iter().enumerate() {
                                self.walk_expr(it, &Use::ToParam(name.clone(), j), enc);
                            }
                            return;
                        }
                    }
                    // `local`/`declare`/`typeset`/`export` name=value args:
                    // each value's `$ref`s flow into the named scalar cell
                    // (`local r=$1` → $1 → cell r).
                    if matches!(name.as_str(), "local" | "declare" | "typeset" | "export") {
                        self.walk_local_args(args, enc);
                        return;
                    }
                }
                // Command words: any `$name` in a string arg may be anything.
                for a in args {
                    self.walk_expr(a, &Use::Text, enc);
                }
            }
            _ => {
                if self.defined_fns.contains(func) {
                    // User function: arg j → callee positional slot j.
                    for (j, a) in args.iter().enumerate() {
                        self.walk_expr(a, &Use::ToParam(func.to_string(), j), enc);
                    }
                } else {
                    // Unknown builtin/runtime call: conservative text.
                    for a in args {
                        self.walk_expr(a, &Use::Text, enc);
                    }
                }
            }
        }
    }

    /// `local`/`declare`/`typeset`/`export` args: `Str("name=value")` cells
    /// (possibly inside Array wrappers) — each value's `$ref`s flow into the
    /// named scalar. A bare `Str("name")` declares without initializing.
    fn walk_local_args(&mut self, args: &[IrExpr], enc: Option<&str>) {
        fn each_str<'a>(e: &'a IrExpr, out: &mut Vec<&'a str>) {
            match e {
                IrExpr::Str(s, _) => out.push(s.as_str()),
                IrExpr::Array(items) => {
                    for it in items {
                        each_str(it, out);
                    }
                }
                _ => {}
            }
        }
        let mut cells: Vec<&str> = Vec::new();
        for a in args.iter().skip(1) {
            each_str(a, &mut cells);
        }
        for cell in cells {
            if let Some((name, value)) = cell.split_once('=') {
                if name.is_empty() {
                    continue;
                }
                self.record_text_expansions(value, &Use::ToCell(name.to_string()), enc);
            }
        }
    }

    /// Numeric expression: every var/ident leaf is an inherently numeric read.
    fn walk_arith(&mut self, a: &ArithAst, enc: Option<&str>) {
        match a {
            ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
            ArithAst::Var(n) | ArithAst::Ident(n) => {
                self.read_name(n, &Use::Num, enc);
            }
            ArithAst::Index { key, .. } => self.walk_arith(key, enc),
            ArithAst::Bin { lhs, rhs, .. } => {
                self.walk_arith(lhs, enc);
                self.walk_arith(rhs, enc);
            }
            ArithAst::Un { arg, .. } => self.walk_arith(arg, enc),
            ArithAst::Cond { test, then, else_ } => {
                self.walk_arith(test, enc);
                self.walk_arith(then, enc);
                self.walk_arith(else_, enc);
            }
            ArithAst::Assign { var, rhs, .. } => {
                self.read_name(var, &Use::Num, enc);
                self.walk_arith(rhs, enc);
            }
            ArithAst::IncDec { var, .. } => self.read_name(var, &Use::Num, enc),
            ArithAst::Cast { arg, .. } => self.walk_arith(arg, enc),
        }
    }
}

/// Extract bare `$name` / `${name…}` references from a text string.
fn dollar_refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'$' {
            i += 1;
            continue;
        }
        i += 1;
        if i < b.len() && b[i] == b'{' {
            i += 1;
            let st = i;
            while i < b.len() && b[i] != b'}' {
                i += 1;
            }
            let inner = &text[st..i.min(b.len())];
            let nm = inner
                .trim_start_matches(['#', '!', '$'])
                .split(['[', ':', '-', '+', '#', '%', '/'])
                .next()
                .unwrap_or("");
            if !nm.is_empty() {
                out.push(nm.to_string());
            }
        } else {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if st != i {
                out.push(text[st..i].to_string());
            }
        }
    }
    out
}

/// Run the analysis over a program.
pub fn analyze(prog: &IrProgram) -> NumericObserved {
    // Pass 0: defined function names (for the interprocedural arg→param edge).
    let mut defined_fns = HashSet::new();
    fn collect_fns(stmts: &[IrStmt], out: &mut HashSet<String>) {
        for s in stmts {
            match s {
                IrStmt::Function { name, body, named_blocks, .. } => {
                    out.insert(name.clone());
                    collect_fns(body, out);
                    for (_, b) in named_blocks {
                        collect_fns(b, out);
                    }
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    collect_fns(then, out);
                    for (_, b) in elsifs {
                        collect_fns(b, out);
                    }
                    collect_fns(else_, out);
                }
                IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => collect_fns(body, out),
                IrStmt::For { body, .. } | IrStmt::Block(body) | IrStmt::Subshell(body)
                | IrStmt::Background(body) => collect_fns(body, out),
                IrStmt::ForInit { init, step, body, .. } => {
                    collect_fns(init, out);
                    collect_fns(step, out);
                    collect_fns(body, out);
                }
                IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
                    collect_fns(body, out);
                    for e in excepts {
                        collect_fns(&e.body, out);
                    }
                    collect_fns(else_body, out);
                    collect_fns(finally_body, out);
                }
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        collect_fns(&c.body, out);
                    }
                }
                IrStmt::Select { clauses } => {
                    for c in clauses {
                        collect_fns(&c.body, out);
                    }
                }
                IrStmt::Redirect { inner, .. } => collect_fns(inner, out),
                IrStmt::Pipeline { stages, .. } => {
                    for st in stages {
                        collect_fns(st, out);
                    }
                }
                _ => {}
            }
        }
    }
    collect_fns(&prog.stmts, &mut defined_fns);
    for sub in &prog.subs {
        defined_fns.insert(sub.name.clone());
    }

    // Pass 1: structural use collection.
    let mut c = Collector {
        defined_fns,
        scalars: HashSet::new(),
        params: HashSet::new(),
        arrays: HashSet::new(),
        scalar_uses: HashMap::new(),
        param_uses: HashMap::new(),
        array_uses: HashMap::new(),
        rest_positionals: HashSet::new(),
    };
    c.walk_stmts(&prog.stmts, None);
    for sub in &prog.subs {
        c.walk_stmts(&sub.body, Some(&sub.name));
    }

    // Post-pass: `$@`/`$*` inside f text-observes every param slot of f
    // (word count is dynamic, so the slots are marked after collection).
    let mut rest_slots: Vec<(String, usize)> = Vec::new();
    for f in &c.rest_positionals {
        for (fname, i) in c.param_uses.keys() {
            if fname == f {
                rest_slots.push((fname.clone(), *i));
            }
        }
    }
    for k in rest_slots {
        if let Some(u) = c.param_uses.get_mut(&k) {
            u.push(Use::Text);
        }
    }

    // Candidate membership.
    let mut scalars = c.scalars;
    let mut params = c.params;
    let mut arrays = c.arrays;

    // Every param slot a body reads (static — feeds spread-served and
    // array_chain; built from the pre-fixpoint candidate keys).
    let mut fn_slots: HashMap<String, Vec<usize>> = HashMap::new();
    for (f, i) in c.param_uses.keys() {
        fn_slots.entry(f.clone()).or_default().push(*i);
    }

    // Greatest fixpoint: removal-only.
    loop {
        let mut drop_scalar: Vec<String> = Vec::new();
        let mut drop_param: Vec<(String, usize)> = Vec::new();
        let mut drop_array: Vec<String> = Vec::new();

        for (v, uses) in &c.scalar_uses {
            if !scalars.contains(v) {
                continue;
            }
            if let Some(bad) = uses.iter().find(|u| !served(u, &scalars, &params, &arrays, &fn_slots)) {
                if debug() {
                    eprintln!("numobs DROP scalar {v}: unserved use {bad:?}");
                }
                drop_scalar.push(v.clone());
            }
        }
        for (k, uses) in &c.param_uses {
            if !params.contains(k) {
                continue;
            }
            if let Some(bad) = uses.iter().find(|u| !served(u, &scalars, &params, &arrays, &fn_slots)) {
                if debug() {
                    eprintln!("numobs DROP param {k:?}: unserved use {bad:?}");
                }
                drop_param.push(k.clone());
            }
        }
        for (a, uses) in &c.array_uses {
            if !arrays.contains(a) {
                continue;
            }
            if let Some(bad) = uses.iter().find(|u| !served(u, &scalars, &params, &arrays, &fn_slots)) {
                if debug() {
                    eprintln!("numobs DROP array {a}: unserved use {bad:?}");
                }
                drop_array.push(a.clone());
            }
        }

        if drop_scalar.is_empty() && drop_param.is_empty() && drop_array.is_empty() {
            break;
        }
        for v in drop_scalar {
            scalars.remove(&v);
        }
        for k in drop_param {
            params.remove(&k);
        }
        for a in drop_array {
            arrays.remove(&a);
        }
    }

    NumericObserved {
        scalars,
        params,
        arrays,
        scalar_uses: c.scalar_uses,
        param_uses: c.param_uses,
        array_uses: c.array_uses,
        fn_slots,
    }
}

/// Is a read's consumption "served" (numeric) under current membership?
fn served(
    u: &Use,
    scalars: &HashSet<String>,
    params: &HashSet<(String, usize)>,
    arrays: &HashSet<String>,
    fn_slots: &HashMap<String, Vec<usize>>,
) -> bool {
    match u {
        Use::Num => true,
        Use::Text => false,
        Use::ToCell(v) => scalars.contains(v),
        Use::ToParam(f, i) => params.contains(&(f.clone(), *i)),
        Use::ToParamSpread(f) => fn_slots
            .get(f)
            .map(|slots| {
                slots
                    .iter()
                    .all(|i| params.contains(&(f.clone(), *i)))
            })
            .unwrap_or(true),
        Use::ToArray(a) => arrays.contains(a),
    }
}

/// `SH2_DEBUG_NUMOBS=1` — print every removal with its unserved use
/// (same env-gate convention as `SH2_DEBUG_PARSER` / `SH2_DEBUG_INT_ELEM`).
fn debug() -> bool {
    std::env::var_os("SH2_DEBUG_NUMOBS").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(src: &str) -> NumericObserved {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir_raw(&cmds);
        analyze(&prog)
    }

    #[test]
    fn atoll_only_scalar_is_numeric() {
        // `v` is written from a substring-ish string and read only in arith.
        let o = obs("v=${p%% *}\necho $(( v * 2 ))\n");
        assert!(o.scalar("v"), "v read only in arith must be numeric-observed");
    }

    #[test]
    fn echo_observed_scalar_is_not_numeric() {
        let o = obs("v=5\necho \"$v\"\n");
        assert!(!o.scalar("v"), "echo observes text");
    }

    #[test]
    fn numeric_sink_param_is_numeric() {
        // f's $1 is only used in arith; caller passes a string temp.
        let src = "f() { local r=$1; echo $(( r * 2 )); }\nt=5\nf \"$t\"\n";
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir_raw(&cmds);
        let o = analyze(&prog);
        assert!(o.param("f", 0), "f's $1 only arith-read → numeric sink (params={:?})", o.params);
        assert!(o.scalar("t"), "t flows only into numeric sink param");
    }

    #[test]
    fn text_sink_param_is_not_numeric() {
        let src = "f() { echo \"$1\"; }\nt=5\nf \"$t\"\n";
        let o = obs(src);
        assert!(!o.param("f", 0), "f's $1 is echoed → text");
        assert!(!o.scalar("t"), "t flows into a text param");
    }

    #[test]
    fn copy_transparency_chain() {
        // a → b → arith : both numeric.
        let o = obs("a=5\nb=$a\necho $(( b + 1 ))\n");
        assert!(o.scalar("a") && o.scalar("b"));
    }

    #[test]
    fn array_read_into_arith_is_numeric() {
        // A is read element-wise into a numeric scalar.
        let src = "A=(1 2 3)\ni=0\nx=${A[$i]}\necho $(( x * 2 ))\n";
        let o = obs(src);
        assert!(o.array("A"), "A element read only into arith → numeric-observed");
    }

    #[test]
    fn array_whole_read_is_not_numeric() {
        let src = "A=(1 2 3)\necho \"${A[@]}\"\n";
        let o = obs(src);
        assert!(!o.array("A"), "whole-array join observes text");
    }

    #[test]
    fn split_spread_gates_on_all_slots() {
        // `f $x` spreads x's words over $1 AND $2: x must not be
        // numeric-observed while $2 is text-observed.
        let src = "f() { local a=$1; echo \"$2\"; echo $(( a + 1 )); }\nx=\"1 2\"\nf $x\n";
        let o = obs(src);
        assert!(!o.scalar("x"), "x feeds a text-observed slot via split spread");
    }

    #[test]
    fn split_spread_ok_when_all_slots_numeric() {
        let src = "f() { local a=$1; local b=$2; echo $(( a + b )); }\nx=\"1 2\"\nf $x\n";
        let o = obs(src);
        assert!(o.scalar("x"), "all slots numeric → spread served");
    }

    #[test]
    fn array_chain_reaches_downstream_scalars() {
        let o = obs("A=(1 2 3)\ni=0\nx=${A[$i]}\ny=$x\necho $(( y + 1 ))\n");
        let c = o.array_chain("A");
        assert!(c.contains("x") && c.contains("y"), "chain: {c:?}");
    }

    /// Run manually against a real A1 dump:
    /// `SH2_A1_JSON=/tmp/a1.json cargo test --lib numeric_observed_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn numeric_observed_real_a1() {
        let Ok(path) = std::env::var("SH2_A1_JSON") else {
            return;
        };
        let text = std::fs::read_to_string(&path).expect("read a1");
        let prog = crate::shir_json_in::shir_json_to_ir(&text).expect("ingest");
        let o = analyze(&prog);
        let mut arrays: Vec<_> = o.arrays.iter().cloned().collect();
        arrays.sort();
        for a in &arrays {
            eprintln!("array candidate {} -> {}", a, o.array(a));
        }
        eprintln!(
            "scalars={} params={} arrays={}",
            o.scalars.len(),
            o.params.len(),
            o.arrays.len()
        );
    }
}
