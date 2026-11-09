// Standalone model of the C backend's DECL-COVERAGE decision
// (sh2perl/src/c_backend.rs) — the three name censuses and the
// predicate they feed:
//
//   INVENTORY  collect_vars_full (≈43501) + collect_vars_expr (≈44730)
//              — the full pass-1 walk. FILE-SCOPE decl emission folds
//              inventory ∪ store; it SKIPS function bodies, Case,
//              Select, Return, Pipeline, Exec, Try (pre-2026-10-28),
//              ForInit-cond/step come via collect_vars; for-loop
//              counters go to for_vars (deliberately NOT declared at
//              file scope).
//   HOIST      collect_assigned_vars (≈44183) + collect_assigned_expr
//              — "everything the hoists emit a C declaration for"
//              (the source's own doc on has_c_decl). Covers the WHOLE
//              program incl. function bodies (fn-local decl ground
//              truth) and Case/Select clause bodies.
//   VETO       collect_param_write_targets + collect_param_write_expr
//              — the const-lift veto: "one precise set" of `${NAME:=…}`
//              write targets. A var it names is not a constant.
//
// All three feed decisions through:
//
//   has_c_decl(name) = var_types ∨ inventory_vars ∨ hoist_vars
//     (≈17861) — THE env-var guard: a read of a var with no C decl
//     renders `_sh_env(name)` (reads the child's environment); with a
//     decl it renders the declared C variable. A WRITE the censuses
//     miss therefore reads back as the wrong value — the fn3 bug
//     (one line where bash prints two, 2026-10-28), the t04 `y`
//     undeclared at cc, and the silent `while`-cond wrong-value diff
//     fixed the same day.
//
// WHY THIS EXISTS. The gate corpus can only show that writes which
// appear in the corpus are covered. This model states the invariant
// for the WHOLE shIR statement/expression grammar:
//
//   COVERAGE:  every node position where bash semantics can assign a
//              variable is collected by the inventory OR the hoist
//              census (⇒ has_c_decl is true ⇒ the read routes to the
//              declared C variable, not to _sh_env).
//   VETO EXACTNESS: the const-lift veto's set is EXACTLY the set of
//              param-write/raw-write positions — sound (never misses a
//              runtime-mutable `:=` target → const-lifting never
//              folds a var that can still change) and precise (never
//              names a read).
//   EV BRIDGE: the shared gate (`param` op ∈ {`=` , `:=`} on a plain
//              identifier) is exactly the DENOTATIONAL write: when it
//              holds, evaluating the node changes the store at that
//              name and nowhere else; when it fails, evaluation
//              leaves the store untouched. The denotation extends the
//              shIR semantics with the bash parameter-assignment rule
//              (`:` assigns when unset OR null; `=` only when unset;
//              word value under quotes = unescape).
//   PINS:      the exact shapes behind the 2026-10-28 fixes (t04
//              param arm, while-cond, raw nested write, fn3 Return,
//              elsif cond, printf -v, array-element guard, select
//              ch/value), asserted positively AND negatively where a
//              walker deliberately does NOT collect (fidelity both
//              ways).
//
// WHAT IS MODELLED (fidelity, checked against the source arm-by-arm;
// MECHANICALLY since 2026-10-29 — harness/declcov-extract.py extracts
// these walkers' arm chains from the live c_backend.rs and the gate
// diffs them against the committed c_decl_coverage_sync.txt, so a
// dropped arm or loosened guard fails loudly — see v19):
//   - every IrStmt arm the three walkers match, with each walker's
//     exact recursion set: e.g. inventory walks ELSIF CONDS but not
//     Case/Select/Function bodies; hoist walks For-iter (since this
//     model's audit), elsif conds, expr-position printf/let (fixes
//     (a)-(d) below); veto walks Try but not Ext nodes.
//   - the exclusive statement-position arms: a stmt-level
//     `printf -v`/`unset`/`read` call's write is its extracted target
//     word ONLY (source does not fall through to the generic arg walk
//     once the guard matches) — mirrored on BOTH the write side and
//     the collector side so exclusivity is not a proof hole.
//   - word flattening for `local x=1` / `unset v` / `read v`: args[1..]
//     with Array items spliced one level, exactly as the source's
//     `skip(1)` + `Array → extend` loop.
//   - `scan_raw_assigns_str`: `${NAME:=w}`/`${NAME=w}` read out of raw
//     Str text (name = alpha/_ + alnum/_ run; `=`/`:=` follows; the
//     preceding `\` escapes; SingleQuoted style is skipped).
//   - is_ident / let_var_name / trim, as spec functions shared by both
//     sides of every theorem (they cancel).
//
// FIXES THIS MODEL'S AUDIT PRODUCED IN SOURCE (2026-10-28, verified by
// the probe battery + C gate, recorded in C_UB_PROOF.md v11b; that
// working tree was later lost and RE-LANDED from this file in 2026-10-
// 29 — C_UB_PROOF.md v18, which also adds the `:=`-default `_sh_xstrdup`
// render fix the probe battery forced):
//   (0) Try arms in BOTH decl censuses (the veto already walked Try;
//       the C backend renders Try via setjmp/longjmp — a write there
//       reached no decl channel at all, not even decl emission).
//   (a) elsif COND walk in the hoist census (inventory + veto had it);
//   (b) For-ITER walk in the hoist census (inventory + veto had it);
//   (c,d) expr-position `printf -v` / `let` target extraction in the
//       hoist census (inventory's expr walk had both).
//   Inside a FUNCTION these four positions were reachable by NO decl
//   channel — inventory skips fn bodies, so hoist alone must cover
//   every fn-scope write, which is exactly the COVERAGE theorem.
//
// WHAT IS NOT MODELLED (carve-outs, each with its verdict):
//   - IrExpr::Capture `$(…)`: bash runs it in a SUBSHELL — writes
//     inside never reach the parent scope, so it is not a parent-store
//     write; statements inside arrive as Arrow bodies, whose writes
//     ARE modelled (the hoist Arrow arm collects them).
//   - IrExpr::Ternary/DefinedOr/Index/MethodCall/Capture/Range/
//     Regex/RawExpr/Splice/Ext/Lambda/ArrayComp/Json/Bool/Ident: no
//     arm exists in ANY of the three walkers (collect_vars_expr falls
//     to `_`). The sh lowering the C backend consumes produces none of
//     them for shell input (measured: shell input lowers to
//     Call/Str/Interp/Array/BinOp/Var/Arith/Arrow); a frontend that
//     emits one carries the obligation in its own node spec
//     (harness/node-spec-check). Excluded from the AST entirely
//     rather than asserted about.
//   - nested `ArithAst::Assign` under Bin/Un/Cond: representable in
//     the type but not produced by any frontend — the shell parser
//     puts assignment at the TOP of an arith expr, and the only
//     nested form measured (`$((z + (w = 2)))` inside larger text)
//     arrives as raw text in `Call{func:"arith"}`, which the C
//     backend defers WHOLESALE to child bash (probe: both sides print
//     `5 2`), so no C-side decl is needed. The model's Arith node
//     therefore carries only a top-level write target (Option).
//   - stmt-position `printf -v`/`unset`/`read` argument words beyond
//     the extracted targets (e.g. a `read -p "${y:=x}"` prompt): the
//     source's arms are exclusive; contrived and untested — recorded,
//     not claimed.
//   - devnull-redirect unwrap: an aliasing pre-step that reveals a
//     bare printf -v; both walkers reach the same shapes through their
//     Redirect/Expr arms anyway (the model's Redirect arm for
//     inventory is `false` — an UNDER-approximation, safe direction).
//   - SH2_CASEWALK_OFF / env guards: assumed unset (debug toggles).
//   - var_types and the `store` census: has_c_decl's third conjunct
//     and decl emission's other half are outside this model; COVERAGE
//     is stated against inventory ∪ hoist alone (stronger), and
//     file-scope decl emission (inventory ∪ store) is established
//     empirically by the probe battery + gate, not here.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_decl_coverage.rs
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;
pub type Name = Str;
pub type Store = Map<Name, Option<Str>>;

// ── string/char helpers (shared by collectors, writes, denotation) ──

// (seq! literals only — push-chains do NOT normalize against them in
// this Verus build, probed; see c_width_store's header note.)
pub open spec fn str_lit(s: Str) -> Str {
    s
}

pub open spec fn digit_c(c: char) -> bool {
    '0' <= c && c <= '9'
}

pub open spec fn alpha_c(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
}

pub open spec fn ident_first_c(c: char) -> bool {
    alpha_c(c) || c == '_'
}

pub open spec fn ident_rest_c(c: char) -> bool {
    alpha_c(c) || digit_c(c) || c == '_'
}

pub open spec fn ws_c(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

// `is_ident`, verbatim: nonempty, first alpha/_, all alnum/_.
pub open spec fn is_ident(x: Str) -> bool {
    x.len() > 0 && ident_first_c(x[0]) && forall|i: int| 0 <= i < x.len() ==> ident_rest_c(x[i])
}

// ASCII whitespace trim (Rust `str::trim` is Unicode; shell words are
// ASCII — the approximation is on both sides of every theorem).
pub open spec fn trim_l(s: Str) -> Str
    decreases s.len(),
{
    if s.len() > 0 && ws_c(s[0]) {
        trim_l(s.drop_first())
    } else {
        s
    }
}

pub open spec fn trim_r(s: Str) -> Str
    decreases s.len(),
{
    if s.len() > 0 && ws_c(s[s.len() - 1]) {
        trim_r(s.subrange(0, s.len() - 1))
    } else {
        s
    }
}

pub open spec fn trim_ws(s: Str) -> Str {
    trim_l(trim_r(s))
}

// Longest leading run of ident-rest chars (= Rust's alnum|_ scan).
pub open spec fn ident_run(s: Str) -> int
    decreases s.len(),
{
    if s.len() > 0 && ident_rest_c(s[0]) {
        1 + ident_run(s.drop_first())
    } else {
        0
    }
}

pub open spec fn strip_incdec(s: Str) -> Str {
    if s.len() >= 2 && s[0] == '+' && s[1] == '+' {
        s.subrange(2, s.len() as int)
    } else if s.len() >= 2 && s[0] == '-' && s[1] == '-' {
        s.subrange(2, s.len() as int)
    } else {
        s
    }
}

// `let_var_name`, verbatim: trim, strip leading ++/--, trim, take the
// alnum/_ prefix (empty → None).
pub open spec fn let_target_of(text: Str) -> Option<Name> {
    let s2 = trim_ws(strip_incdec(trim_ws(text)));
    let k = ident_run(s2);
    if k > 0 {
        Some(s2.subrange(0, k))
    } else {
        None
    }
}

// Least index of '=' (Rust `split_once('=')`).
pub open spec fn eq_pos(s: Str) -> Option<int>
    decreases s.len(),
{
    if s.len() == 0 {
        None
    } else if s[0] == '=' {
        Some(0)
    } else {
        match eq_pos(s.drop_first()) {
            Some(j) => Some(j + 1),
            None => None,
        }
    }
}

// `local x=1` word target: prefix before the FIRST '=', nonempty,
// all alnum/_ (no first-char restriction — verbatim).
pub open spec fn decl_word_target(w: Str) -> Option<Name> {
    match eq_pos(w) {
        Some(j) => {
            if j > 0 && forall|k: int| 0 <= k < j ==> ident_rest_c(w[k]) {
                Some(w.subrange(0, j))
            } else {
                None
            }
        },
        None => None,
    }
}

// ── call-shape predicates (each mirrors one source guard) ───────────

pub open spec fn c_exec() -> Str {
    seq!['e', 'x', 'e', 'c']
}

pub open spec fn c_builtin() -> Str {
    seq!['b', 'u', 'i', 'l', 't', 'i', 'n']
}

pub open spec fn c_param() -> Str {
    seq!['p', 'a', 'r', 'a', 'm']
}

pub open spec fn is_ext(func: Str) -> bool {
    func == c_exec() || func == c_builtin()
}

pub open spec fn arg_text(a: Seq<Expr>, i: int) -> Option<Str> {
    if 0 <= i && i < a.len() {
        match a[i] {
            Expr::Str { text, .. } => Some(text),
            _ => None,
        }
    } else {
        None
    }
}

pub open spec fn arg_pair(a: Seq<Expr>, i: int) -> Option<(Str, bool)> {
    if 0 <= i && i < a.len() {
        match a[i] {
            Expr::Str { text, single } => Some((text, single)),
            _ => None,
        }
    } else {
        None
    }
}

pub open spec fn cmd_is(a: Seq<Expr>, c: Str) -> bool {
    arg_text(a, 0) == Some(c)
}

// is_printf_v_call: args[0] Str "printf", args[1] Array whose first
// item is Str "-v". Target-independent (the source arm is exclusive).
pub open spec fn printf_shape(a: Seq<Expr>) -> bool {
    a.len() >= 2
        && cmd_is(a, seq!['p', 'r', 'i', 'n', 't', 'f'])
        && match a[1] {
            Expr::Arr { items } => items.len() >= 1 && arg_text(items, 0) == Some(seq!['-', 'v']),
            _ => false,
        }
}

pub open spec fn printf_v_target(a: Seq<Expr>) -> Option<Name> {
    if printf_shape(a) {
        match a[1] {
            Expr::Arr { items } => arg_text(items, 1),
            _ => None,
        }
    } else {
        None
    }
}

// `let` arm guard: ext + args[0] "let" + args[1] Array with Str first.
pub open spec fn let_shape(a: Seq<Expr>) -> bool {
    a.len() >= 2
        && cmd_is(a, seq!['l', 'e', 't'])
        && match a[1] {
            Expr::Arr { items } => items.len() >= 1 && arg_text(items, 0).is_some(),
            _ => false,
        }
}

pub open spec fn let_target(a: Seq<Expr>) -> Option<Name> {
    if let_shape(a) {
        match a[1] {
            Expr::Arr { items } => {
                match arg_text(items, 0) {
                    Some(t) => let_target_of(t),
                    None => None,
                }
            },
            _ => None,
        }
    } else {
        None
    }
}

// exec-declare guard: args[0] ∈ {local,declare,typeset,export,readonly}.
pub open spec fn is_declare_cmd(a: Seq<Expr>) -> bool {
    cmd_is(a, seq!['l', 'o', 'c', 'a', 'l'])
        || cmd_is(a, seq!['d', 'e', 'c', 'l', 'a', 'r', 'e'])
        || cmd_is(a, seq!['t', 'y', 'p', 'e', 's', 'e', 't'])
        || cmd_is(a, seq!['e', 'x', 'p', 'o', 'r', 't'])
        || cmd_is(a, seq!['r', 'e', 'a', 'd', 'o', 'n', 'l', 'y'])
}

pub open spec fn is_unset_cmd(a: Seq<Expr>) -> bool {
    cmd_is(a, seq!['u', 'n', 's', 'e', 't'])
}

pub open spec fn is_read_cmd(a: Seq<Expr>) -> bool {
    cmd_is(a, seq!['r', 'e', 'a', 'd'])
}

// THE shared param gate: op ∈ {`=` , `:=`} on a plain identifier.
pub open spec fn is_write_op(op: Str) -> bool {
    op == seq!['='] || op == seq![':', '=']
}

pub open spec fn param_gate(a: Seq<Expr>) -> Option<Name> {
    match (arg_text(a, 0), arg_text(a, 1)) {
        (Some(op), Some(name)) => {
            if is_write_op(op) && is_ident(name) {
                Some(name)
            } else {
                None
            }
        },
        (_, _) => None,
    }
}

// ── word extraction (args[1..] with one-level Array splice) ─────────

pub open spec fn flatten_words(a: Seq<Expr>, i: int) -> Seq<Expr>
    decreases a.len() - i,
{
    if i < 0 || i >= a.len() {
        Seq::empty()
    } else {
        let rest = flatten_words(a, i + 1);
        match a[i] {
            Expr::Arr { items } => items.add(rest),
            other => seq![other].add(rest),
        }
    }
}

pub enum WScan {
    Decl,
    Unset,
    Read,
}

pub open spec fn word_hits(w: Expr, k: WScan, n: Name) -> bool {
    match w {
        Expr::Str { text, .. } => {
            match k {
                WScan::Decl => decl_word_target(text) == Some(n),
                WScan::Unset => {
                    let u = trim_ws(text);
                    u == n && u.len() > 0 && forall|i: int| 0 <= i < u.len() ==> ident_rest_c(u[i])
                },
                WScan::Read => text == n && !starts_dash(text) && is_ident(n),
            }
        },
        _ => false,
    }
}

pub open spec fn starts_dash(t: Str) -> bool {
    t.len() > 0 && t[0] == '-'
}

pub open spec fn word_scan(a: Seq<Expr>, k: WScan, n: Name) -> bool {
    let ws = flatten_words(a, 1);
    exists|i: int| 0 <= i < ws.len() && word_hits(ws[i], k, n)
}

// ── raw-text writes: `scan_raw_assigns_str` ─────────────────────────

pub open spec fn raw_hit(text: Str, n: Name, i: int) -> bool {
    if !(i >= 0 && i + 2 < text.len()) {
        false
    } else if !(text[i] == '$' && text[i + 1] == '{') {
        false
    } else if i > 0 && text[i - 1] == '\\' {
        false
    } else if !ident_first_c(text[i + 2]) {
        false
    } else {
        let k = ident_run(text.subrange(i + 2, text.len() as int));
        k > 0
            && text.subrange(i + 2, i + 2 + k) == n
            && ((i + 2 + k < text.len() && text[i + 2 + k] == '=') || (i + 3 + k < text.len()
                && text[i + 2 + k] == ':'
                && text[i + 3 + k] == '='))
    }
}

pub open spec fn raw_hits(text: Str, n: Name) -> bool {
    exists|i: int| raw_hit(text, n, i)
}

// ── the AST (walk-relevant subset — see carve-outs in the header) ───

pub enum Expr {
    Lit,
    Str { text: Str, single: bool },
    Var(Name),
    Bin { l: Box<Expr>, r: Box<Expr> },
    Call { func: Str, args: Seq<Expr> },
    Interp { parts: Seq<Expr> },
    Arr { items: Seq<Expr> },
    Arrow { body: Seq<Stmt> },
    // ArithAst abstracted to its walk-relevant content: Some(var) iff
    // the TOP level is Assign|IncDec (hoist inserts exactly that; the
    // inventory's arith walk collects only Var READS, which are never
    // writes — modelled as false, an under-approximation).
    Arith { write: Option<Name> },
}

pub struct Clause {
    pub ch: Option<Expr>,
    pub value: Option<Expr>,
    pub body: Seq<Stmt>,
}

pub struct Except {
    pub match_expr: Option<Expr>,
    pub body: Seq<Stmt>,
}

pub enum Stmt {
    Assign { targets: Seq<Name>, expr: Expr },
    Declare { vars: Seq<Name>, init: Option<Expr> },
    DeclareArray { elements: Seq<Expr> },
    Expr(Expr),
    Output(Expr),
    Return(Option<Expr>),
    Exit(Option<Expr>),
    Die(Expr),
    Warn(Expr),
    SetChildError(Expr),
    WriteFile { path: Expr, content: Expr },
    Exec { cmd: Expr, args: Seq<Expr>, redirects: Seq<Expr>, env: Seq<Expr> },
    Redirect { inner: Seq<Stmt>, targets: Seq<Expr> },
    Pipeline { stages: Seq<Seq<Stmt>> },
    If { cond: Expr, then: Seq<Stmt>, elsifs: Seq<(Expr, Seq<Stmt>)>, else_: Seq<Stmt> },
    Case { discriminant: Expr, bodies: Seq<Seq<Stmt>> },
    Select { clauses: Seq<Clause> },
    While { cond: Expr, body: Seq<Stmt> },
    DoWhile { cond: Expr, body: Seq<Stmt> },
    For { var: Name, iter: Expr, body: Seq<Stmt> },
    ForInit { init: Seq<Stmt>, cond: Expr, step: Seq<Stmt>, body: Seq<Stmt> },
    Function { body: Seq<Stmt> },
    Try { body: Seq<Stmt>, excepts: Seq<Except>, else_body: Seq<Stmt>, finally_body: Seq<Stmt> },
    // Block | Subshell | Background — one or-pattern arm in every walker.
    Scope(Seq<Stmt>),
    ExtTakeField { x: Name, f: Name },
}

// ── sizes (termination measures for the walkers AND the proofs) ─────

pub open spec fn esize(a: Seq<Expr>) -> int
    decreases a,
{
    if a.len() == 0 {
        0
    } else {
        e_size(a[0]) + esize(a.drop_first()) + 1
    }
}

pub open spec fn opt_e_size(e: Option<Expr>) -> int
    decreases e,
{
    match e {
        Some(x) => e_size(x),
        None => 0,
    }
}

pub open spec fn seq_size(b: Seq<Stmt>) -> int
    decreases b,
{
    if b.len() == 0 {
        0
    } else {
        stmt_size(b[0]) + seq_size(b.drop_first()) + 1
    }
}

pub open spec fn eifs_size(c: Seq<(Expr, Seq<Stmt>)>) -> int
    decreases c,
{
    if c.len() == 0 {
        0
    } else {
        e_size(c[0].0) + seq_size(c[0].1) + eifs_size(c.drop_first()) + 1
    }
}

pub open spec fn stmt_size(s: Stmt) -> int
    decreases s,
{
    match s {
        Stmt::Assign { expr, .. } => 1 + e_size(expr),
        Stmt::Declare { init, .. } => 1 + opt_e_size(init),
        Stmt::DeclareArray { elements } => 1 + esize(elements),
        Stmt::Expr(e) | Stmt::Output(e) | Stmt::Die(e) | Stmt::Warn(e) | Stmt::SetChildError(e) => {
            1 + e_size(e)
        },
        Stmt::Return(e) | Stmt::Exit(e) => 1 + opt_e_size(e),
        Stmt::WriteFile { path, content } => 1 + e_size(path) + e_size(content),
        Stmt::Exec { cmd, args, redirects, env } => {
            1 + e_size(cmd) + esize(args) + esize(redirects) + esize(env)
        },
        Stmt::Redirect { inner, targets } => 1 + seq_size(inner) + esize(targets),
        Stmt::Pipeline { stages } => 1 + seq_size_bodies(stages),
        Stmt::If { cond, then, elsifs, else_ } => {
            1 + e_size(cond) + seq_size(then) + eifs_size(elsifs) + seq_size(else_)
        },
        Stmt::Case { discriminant, bodies } => {
            1 + e_size(discriminant) + seq_size_bodies(bodies)
        },
        Stmt::Select { clauses } => 1 + seq_size_clauses(clauses),
        Stmt::While { cond, body } | Stmt::DoWhile { cond, body } => {
            1 + e_size(cond) + seq_size(body)
        },
        Stmt::For { iter, body, .. } => 1 + e_size(iter) + seq_size(body),
        Stmt::ForInit { init, cond, step, body } => {
            1 + seq_size(init) + e_size(cond) + seq_size(step) + seq_size(body)
        },
        Stmt::Function { body } => 1 + seq_size(body),
        Stmt::Try { body, excepts, else_body, finally_body } => {
            1 + seq_size(body) + seq_size_excepts(excepts) + seq_size(else_body)
                + seq_size(finally_body)
        },
        Stmt::Scope(b) => 1 + seq_size(b),
        Stmt::ExtTakeField { .. } => 1,
    }
}

pub open spec fn seq_size_bodies(bs: Seq<Seq<Stmt>>) -> int
    decreases bs,
{
    if bs.len() == 0 {
        0
    } else {
        seq_size(bs[0]) + seq_size_bodies(bs.drop_first()) + 1
    }
}

pub open spec fn seq_size_clauses(cs: Seq<Clause>) -> int
    decreases cs,
{
    if cs.len() == 0 {
        0
    } else {
        opt_e_size(cs[0].ch) + opt_e_size(cs[0].value) + seq_size(cs[0].body)
            + seq_size_clauses(cs.drop_first()) + 1
    }
}

pub open spec fn seq_size_excepts(es: Seq<Except>) -> int
    decreases es,
{
    if es.len() == 0 {
        0
    } else {
        opt_e_size(es[0].match_expr) + seq_size(es[0].body) + seq_size_excepts(es.drop_first()) + 1
    }
}

pub open spec fn e_size(e: Expr) -> int
    decreases e,
{
    match e {
        Expr::Lit | Expr::Var(_) | Expr::Str { .. } | Expr::Arith { .. } => 1,
        Expr::Bin { l, r } => 1 + e_size(*l) + e_size(*r),
        Expr::Call { args, .. } => 1 + esize(args),
        Expr::Interp { parts } | Expr::Arr { items: parts } => 1 + esize(parts),
        Expr::Arrow { body } => 1 + seq_size(body),
    }
}

// ── the collectors (mode-parameterised over the three walks) ────────

pub enum Ch {
    Inv,
    Hoist,
    Veto,
}

pub open spec fn seq_has(a: Seq<Name>, n: Name) -> bool {
    exists|i: int| 0 <= i < a.len() && a[i] == n
}

// Statement-position write shapes, inlined into writes_s — the
// exclusive stmt arms of the source (guarded, no generic fallthrough
// until every guard is false).
pub open spec fn writes_seq(b: Seq<Stmt>, n: Name) -> bool
    decreases b,
{
    exists|i: int| 0 <= i < b.len() && writes_s(b[i], n)
}

pub open spec fn col_seq(b: Seq<Stmt>, n: Name, m: Ch) -> bool
    decreases b,
{
    exists|i: int| 0 <= i < b.len() && col_s(b[i], n, m)
}

pub open spec fn writes_x(a: Seq<Expr>, n: Name) -> bool
    decreases a,
{
    exists|i: int| 0 <= i < a.len() && writes_e(a[i], n)
}

pub open spec fn col_x(a: Seq<Expr>, n: Name, m: Ch) -> bool
    decreases a,
{
    exists|i: int| 0 <= i < a.len() && col_e(a[i], n, m)
}

pub open spec fn opt_w(e: Option<Expr>, n: Name) -> bool
    decreases e,
{
    match e {
        Some(x) => writes_e(x, n),
        None => false,
    }
}

pub open spec fn opt_c(e: Option<Expr>, n: Name, m: Ch) -> bool
    decreases e,
{
    match e {
        Some(x) => col_e(x, n, m),
        None => false,
    }
}


pub open spec fn col_e(e: Expr, n: Name, m: Ch) -> bool
    decreases e,
{
    match e {
        Expr::Lit => false,
        Expr::Str { text, single } => !single && raw_hits(text, n),
        Expr::Var(x) => m == Ch::Inv && x == n,
        Expr::Bin { l, r } => col_e(*l, n, m) || col_e(*r, n, m),
        Expr::Arith { write } => m == Ch::Hoist && write == Some(n),
        Expr::Interp { parts } => col_x(parts, n, m),
        Expr::Arr { items } => col_x(items, n, m),
        Expr::Arrow { body } => {
            if m == Ch::Inv {
                false
            } else {
                col_seq(body, n, m)
            }
        },
        // the per-mode call chains (collect_vars_expr / the hoist
        // expr arm / collect_param_write_expr), inlined: each mode's
        // extraction set is exactly its source's arm set.
        Expr::Call { func, args } => {
            if m == Ch::Inv {
                (is_ext(func) && printf_shape(args) && printf_v_target(args) == Some(n))
                    || (is_ext(func) && let_shape(args) && let_target(args) == Some(n))
                    || (func == c_param() && param_gate(args) == Some(n)) || col_x(args, n, m)
            } else if m == Ch::Hoist {
                if is_ext(func) && is_declare_cmd(args) {
                    word_scan(args, WScan::Decl, n)
                } else if is_ext(func) && is_unset_cmd(args) {
                    word_scan(args, WScan::Unset, n)
                } else if is_ext(func) && printf_shape(args) {
                    printf_v_target(args) == Some(n) || col_x(args, n, m)
                } else if is_ext(func) && let_shape(args) {
                    let_target(args) == Some(n) || col_x(args, n, m)
                } else if func == c_param() {
                    param_gate(args) == Some(n) || col_x(args, n, m)
                } else {
                    col_x(args, n, m)
                }
            } else {
                if func == c_param() {
                    param_gate(args) == Some(n) || col_x(args, n, m)
                } else {
                    col_x(args, n, m)
                }
            }
        },
    }
}

pub open spec fn col_s(s: Stmt, n: Name, m: Ch) -> bool
    decreases s,
{
    match s {
        Stmt::Assign { targets, expr } => {
            if m == Ch::Veto {
                col_e(expr, n, m)
            } else {
                seq_has(targets, n) || col_e(expr, n, m)
            }
        },
        Stmt::Declare { vars, init } => {
            if m == Ch::Veto {
                opt_c(init, n, m)
            } else {
                seq_has(vars, n) || opt_c(init, n, m)
            }
        },
        Stmt::DeclareArray { elements } => {
            if m == Ch::Inv {
                false
            } else {
                col_x(elements, n, m)
            }
        },
        Stmt::Expr(e) => {
            if m == Ch::Hoist {
                // collect_assigned_vars' statement-position arms,
                // inlined: the guarded printf/unset/read arms are
                // exclusive (no generic fallthrough once a guard
                // matches), then the generic expr walk.
                match e {
                    Expr::Call { func, args } => {
                        if printf_shape(args) {
                            printf_v_target(args) == Some(n)
                        } else if is_ext(func) && is_unset_cmd(args) {
                            word_scan(args, WScan::Unset, n)
                        } else if is_ext(func) && is_read_cmd(args) {
                            word_scan(args, WScan::Read, n)
                        } else {
                            col_e(e, n, m)
                        }
                    },
                    _ => col_e(e, n, m),
                }
            } else {
                col_e(e, n, m)
            }
        },
        Stmt::Output(e) => col_e(e, n, m),
        Stmt::Return(e) => {
            if m == Ch::Inv {
                false
            } else {
                opt_c(e, n, m)
            }
        },
        Stmt::Exit(e) => opt_c(e, n, m),
        Stmt::Die(e) => {
            if m == Ch::Inv {
                false
            } else {
                col_e(e, n, m)
            }
        },
        Stmt::Warn(e) => {
            if m == Ch::Inv {
                false
            } else {
                col_e(e, n, m)
            }
        },
        Stmt::SetChildError(e) => {
            if m == Ch::Inv {
                false
            } else {
                col_e(e, n, m)
            }
        },
        Stmt::WriteFile { path, content } => {
            if m == Ch::Inv {
                false
            } else {
                col_e(path, n, m) || col_e(content, n, m)
            }
        },
        Stmt::Exec { cmd, args, redirects, env } => {
            if m == Ch::Inv {
                false
            } else {
                col_e(cmd, n, m) || col_x(args, n, m) || col_x(redirects, n, m)
                    || col_x(env, n, m)
            }
        },
        Stmt::Redirect { inner, targets } => {
            if m == Ch::Inv {
                false
            } else {
                col_seq(inner, n, m) || col_x(targets, n, m)
            }
        },
        Stmt::Pipeline { stages } => {
            if m == Ch::Inv {
                false
            } else {
                exists|i: int| 0 <= i < stages.len() && col_seq(stages[i], n, m)
            }
        },
        Stmt::If { cond, then, elsifs, else_ } => {
            col_e(cond, n, m) || col_seq(then, n, m)
                || exists|i: int|
                    0 <= i < elsifs.len() && (col_e(elsifs[i].0, n, m) || col_seq(elsifs[i].1, n, m))
                || col_seq(else_, n, m)
        },
        Stmt::Case { discriminant, bodies } => {
            if m == Ch::Inv {
                false
            } else {
                col_e(discriminant, n, m)
                    || exists|i: int| 0 <= i < bodies.len() && col_seq(bodies[i], n, m)
            }
        },
        Stmt::Select { clauses } => {
            if m == Ch::Inv {
                false
            } else {
                exists|i: int|
                    0 <= i < clauses.len() && (opt_c(clauses[i].ch, n, m)
                        || opt_c(clauses[i].value, n, m) || col_seq(clauses[i].body, n, m))
            }
        },
        Stmt::While { cond, body } => col_e(cond, n, m) || col_seq(body, n, m),
        Stmt::DoWhile { cond, body } => col_e(cond, n, m) || col_seq(body, n, m),
        Stmt::For { var, iter, body } => {
            if m == Ch::Hoist {
                n == var || col_e(iter, n, m) || col_seq(body, n, m)
            } else {
                // inventory routes the loop var to for_vars (NOT the
                // declared set); the veto never collects loop vars.
                col_e(iter, n, m) || col_seq(body, n, m)
            }
        },
        Stmt::ForInit { init, cond, step, body } => {
            col_seq(init, n, m) || col_e(cond, n, m) || col_seq(step, n, m)
                || col_seq(body, n, m)
        },
        Stmt::Function { body } => {
            if m == Ch::Inv {
                false
            } else {
                col_seq(body, n, m)
            }
        },
        Stmt::Try { body, excepts, else_body, finally_body } => {
            col_seq(body, n, m)
                || exists|i: int|
                    0 <= i < excepts.len() && (opt_c(excepts[i].match_expr, n, m)
                        || col_seq(excepts[i].body, n, m)) || col_seq(else_body, n, m)
                || col_seq(finally_body, n, m)
        },
        Stmt::Scope(b) => col_seq(b, n, m),
        Stmt::ExtTakeField { x, f } => {
            if m == Ch::Veto {
                false
            } else {
                n == x || n == f
            }
        },
    }
}

// ── THE WRITE SIDE (bash denotation of writes) ──────────────────────

pub open spec fn writes_e(e: Expr, n: Name) -> bool
    decreases e,
{
    match e {
        Expr::Lit | Expr::Var(_) => false,
        Expr::Str { text, single } => !single && raw_hits(text, n),
        Expr::Arith { write } => write == Some(n),
        Expr::Bin { l, r } => writes_e(*l, n) || writes_e(*r, n),
        Expr::Interp { parts } => writes_x(parts, n),
        Expr::Arr { items } => writes_x(items, n),
        Expr::Arrow { body } => writes_seq(body, n),
        Expr::Call { func, args } => {
            // exclusive chains mirror the render/collector arms: a
            // declare/unset shape's write is its extracted word targets
            // (those source arms do not recurse into args); printf/let/
            // param extract AND recurse (source does both).
            if is_ext(func) && is_declare_cmd(args) {
                word_scan(args, WScan::Decl, n)
            } else if is_ext(func) && is_unset_cmd(args) {
                word_scan(args, WScan::Unset, n)
            } else if is_ext(func) && printf_shape(args) {
                printf_v_target(args) == Some(n) || writes_x(args, n)
            } else if is_ext(func) && let_shape(args) {
                let_target(args) == Some(n) || writes_x(args, n)
            } else if func == c_param() {
                param_gate(args) == Some(n) || writes_x(args, n)
            } else {
                writes_x(args, n)
            }
        },
    }
}

pub open spec fn writes_s(s: Stmt, n: Name) -> bool
    decreases s,
{
    match s {
        Stmt::Assign { targets, expr } => seq_has(targets, n) || writes_e(expr, n),
        Stmt::Declare { vars, init } => seq_has(vars, n) || opt_w(init, n),
        Stmt::DeclareArray { elements } => writes_x(elements, n),
        Stmt::Expr(e) => {
            // collect_assigned_vars' stmt-position arms, inlined and
            // exclusive: printf -v / unset / read contribute their
            // extracted target words; only when every guard is false
            // does the generic expr walk apply.
            match e {
                Expr::Call { func, args } => {
                    if printf_shape(args) {
                        printf_v_target(args) == Some(n)
                    } else if is_ext(func) && is_unset_cmd(args) {
                        word_scan(args, WScan::Unset, n)
                    } else if is_ext(func) && is_read_cmd(args) {
                        word_scan(args, WScan::Read, n)
                    } else {
                        writes_e(e, n)
                    }
                },
                _ => writes_e(e, n),
            }
        },
        Stmt::Output(e) => writes_e(e, n),
        Stmt::Die(e) => writes_e(e, n),
        Stmt::Warn(e) => writes_e(e, n),
        Stmt::SetChildError(e) => writes_e(e, n),
        Stmt::Return(e) => opt_w(e, n),
        Stmt::Exit(e) => opt_w(e, n),
        Stmt::WriteFile { path, content } => writes_e(path, n) || writes_e(content, n),
        Stmt::Exec { cmd, args, redirects, env } => {
            writes_e(cmd, n) || writes_x(args, n) || writes_x(redirects, n) || writes_x(env, n)
        },
        Stmt::Redirect { inner, targets } => writes_seq(inner, n) || writes_x(targets, n),
        Stmt::Pipeline { stages } => {
            exists|i: int| 0 <= i < stages.len() && writes_seq(stages[i], n)
        },
        Stmt::If { cond, then, elsifs, else_ } => {
            writes_e(cond, n) || writes_seq(then, n)
                || exists|i: int|
                    0 <= i < elsifs.len()
                        && (writes_e(elsifs[i].0, n) || writes_seq(elsifs[i].1, n))
                || writes_seq(else_, n)
        },
        Stmt::Case { discriminant, bodies } => {
            writes_e(discriminant, n)
                || exists|i: int| 0 <= i < bodies.len() && writes_seq(bodies[i], n)
        },
        Stmt::Select { clauses } => {
            exists|i: int|
                0 <= i < clauses.len()
                    && (opt_w(clauses[i].ch, n) || opt_w(clauses[i].value, n)
                        || writes_seq(clauses[i].body, n))
        },
        Stmt::While { cond, body } => writes_e(cond, n) || writes_seq(body, n),
        Stmt::DoWhile { cond, body } => writes_e(cond, n) || writes_seq(body, n),
        Stmt::For { var, iter, body } => n == var || writes_e(iter, n) || writes_seq(body, n),
        Stmt::ForInit { init, cond, step, body } => {
            writes_seq(init, n) || writes_e(cond, n) || writes_seq(step, n)
                || writes_seq(body, n)
        },
        Stmt::Function { body } => writes_seq(body, n),
        Stmt::Try { body, excepts, else_body, finally_body } => {
            writes_seq(body, n)
                || exists|i: int|
                    0 <= i < excepts.len()
                        && (opt_w(excepts[i].match_expr, n) || writes_seq(excepts[i].body, n))
                || writes_seq(else_body, n) || writes_seq(finally_body, n)
        },
        Stmt::Scope(b) => writes_seq(b, n),
        Stmt::ExtTakeField { x, f } => n == x || n == f,
    }
}

// ── the param-write side of the veto exactness claim ────────────────
// Positions where a `${NAME:=…}`/raw-text write can occur — derived
// from the WRITE semantics, structurally independent of the veto's
// code, and claimed to coincide with it exactly.

pub open spec fn paramw_x(a: Seq<Expr>, n: Name) -> bool
    decreases a,
{
    exists|i: int| 0 <= i < a.len() && paramw_e(a[i], n)
}

pub open spec fn paramw_seq(b: Seq<Stmt>, n: Name) -> bool
    decreases b,
{
    exists|i: int| 0 <= i < b.len() && paramw_s(b[i], n)
}

pub open spec fn paramw_opt(e: Option<Expr>, n: Name) -> bool
    decreases e,
{
    match e {
        Some(x) => paramw_e(x, n),
        None => false,
    }
}

pub open spec fn paramw_e(e: Expr, n: Name) -> bool
    decreases e,
{
    match e {
        Expr::Str { text, single } => !single && raw_hits(text, n),
        Expr::Call { func, args } => {
            if func == c_param() {
                param_gate(args) == Some(n) || paramw_x(args, n)
            } else {
                paramw_x(args, n)
            }
        },
        Expr::Bin { l, r } => paramw_e(*l, n) || paramw_e(*r, n),
        Expr::Interp { parts } => paramw_x(parts, n),
        Expr::Arr { items } => paramw_x(items, n),
        Expr::Arrow { body } => paramw_seq(body, n),
        Expr::Lit | Expr::Var(_) | Expr::Arith { .. } => false,
    }
}

pub open spec fn paramw_s(s: Stmt, n: Name) -> bool
    decreases s,
{
    match s {
        Stmt::Assign { expr, .. } => paramw_e(expr, n),
        Stmt::Declare { init, .. } => paramw_opt(init, n),
        Stmt::DeclareArray { elements } => paramw_x(elements, n),
        Stmt::Expr(e) => paramw_e(e, n),
        Stmt::Output(e) => paramw_e(e, n),
        Stmt::Die(e) => paramw_e(e, n),
        Stmt::Warn(e) => paramw_e(e, n),
        Stmt::SetChildError(e) => paramw_e(e, n),
        Stmt::Return(e) => paramw_opt(e, n),
        Stmt::Exit(e) => paramw_opt(e, n),
        Stmt::WriteFile { path, content } => paramw_e(path, n) || paramw_e(content, n),
        Stmt::Exec { cmd, args, redirects, env } => {
            paramw_e(cmd, n) || paramw_x(args, n) || paramw_x(redirects, n) || paramw_x(env, n)
        },
        Stmt::Redirect { inner, targets } => paramw_seq(inner, n) || paramw_x(targets, n),
        Stmt::Pipeline { stages } => {
            exists|i: int| 0 <= i < stages.len() && paramw_seq(stages[i], n)
        },
        Stmt::If { cond, then, elsifs, else_ } => {
            paramw_e(cond, n) || paramw_seq(then, n)
                || exists|i: int|
                    0 <= i < elsifs.len()
                        && (paramw_e(elsifs[i].0, n) || paramw_seq(elsifs[i].1, n))
                || paramw_seq(else_, n)
        },
        Stmt::Case { discriminant, bodies } => {
            paramw_e(discriminant, n)
                || exists|i: int| 0 <= i < bodies.len() && paramw_seq(bodies[i], n)
        },
        Stmt::Select { clauses } => {
            exists|i: int|
                0 <= i < clauses.len()
                    && (paramw_opt(clauses[i].ch, n) || paramw_opt(clauses[i].value, n)
                        || paramw_seq(clauses[i].body, n))
        },
        Stmt::While { cond, body } => paramw_e(cond, n) || paramw_seq(body, n),
        Stmt::DoWhile { cond, body } => paramw_e(cond, n) || paramw_seq(body, n),
        Stmt::For { iter, body, .. } => paramw_e(iter, n) || paramw_seq(body, n),
        Stmt::ForInit { init, cond, step, body } => {
            paramw_seq(init, n) || paramw_e(cond, n) || paramw_seq(step, n)
                || paramw_seq(body, n)
        },
        Stmt::Function { body } => paramw_seq(body, n),
        Stmt::Try { body, excepts, else_body, finally_body } => {
            paramw_seq(body, n)
                || exists|i: int|
                    0 <= i < excepts.len()
                        && (paramw_opt(excepts[i].match_expr, n)
                            || paramw_seq(excepts[i].body, n)) || paramw_seq(else_body, n)
                || paramw_seq(finally_body, n)
        },
        Stmt::Scope(b) => paramw_seq(b, n),
        Stmt::ExtTakeField { .. } => false,
    }
}

// ── THE DENOTATION (ev): bash parameter assignment ──────────────────

pub open spec fn lookup(s: Store, n: Name) -> Option<Str> {
    if s.dom().contains(n) {
        s[n]
    } else {
        None
    }
}

// Word value: a single-quoted word is literal; otherwise backslash
// removes its special meaning before $ ` " \ and newline (the
// unescape_dq convention shared with the string models).
pub open spec fn unescape_dq(t: Str) -> Str
    decreases t.len(),
{
    if t.len() < 2 {
        t
    } else if t[0] == '\\'
        && (t[1] == '$' || t[1] == '`' || t[1] == '"' || t[1] == '\\' || t[1] == '\n')
    {
        seq![t[1]].add(unescape_dq(t.subrange(2, t.len() as int)))
    } else {
        seq![t[0]].add(unescape_dq(t.subrange(1, t.len() as int)))
    }
}

pub open spec fn ev_word(text: Str, single: bool) -> Str {
    if single {
        text
    } else {
        unescape_dq(text)
    }
}

// The store AFTER evaluating a parameter node. The gate is the only
// write path: a writing op on a plain identifier assigns the default
// word when the name is unset (`=`) or unset-or-null (`:=`); every
// other node leaves the store untouched.
pub open spec fn ev_param_store(a: Seq<Expr>, s0: Store) -> Store {
    match param_gate(a) {
        Some(name) => {
            let op = match arg_text(a, 0) {
                Some(o) => o,
                None => Seq::empty(),
            };
            let wordv = match arg_pair(a, 2) {
                Some((w, sq)) => ev_word(w, sq),
                None => Seq::empty(),
            };
            let cur = lookup(s0, name);
            if op == seq![':', '='] {
                match cur {
                    None => s0.insert(name, Some(wordv)),
                    Some(x) => {
                        if x == Seq::empty() {
                            s0.insert(name, Some(wordv))
                        } else {
                            s0
                        }
                    },
                }
            } else {
                match cur {
                    None => s0.insert(name, Some(wordv)),
                    Some(_) => s0,
                }
            }
        },
        None => s0,
    }
}

// ══════════════════════════════════════════════════════════════════
// THE OBLIGATIONS
// ══════════════════════════════════════════════════════════════════

// COVERAGE (1/4 — statements): every write position is collected by
// the HOIST census (fn-local decl ground truth — inventory skips fn
// bodies, so hoist alone must cover every write; inventory ∨ hoist is
// then immediate for has_c_decl).
proof fn coverage_s(s: Stmt, n: Name)
    ensures writes_s(s, n) ==> col_s(s, n, Ch::Hoist),
    decreases s,
{
    if writes_s(s, n) {
        match s {
            Stmt::Assign { targets, expr } => {
                if seq_has(targets, n) {
                } else if writes_e(expr, n) {
                    coverage_e(expr, n);
                }
            },
            Stmt::Declare { vars, init } => {
                if seq_has(vars, n) {
                } else if opt_w(init, n) {
                    match init {
                        Some(x) => coverage_e(x, n),
                        None => {},
                    }
                }
            },
            Stmt::DeclareArray { elements } => {
                if writes_x(elements, n) {
                    let i = choose|i: int| 0 <= i < elements.len() && writes_e(elements[i], n);
                    coverage_e(elements[i], n);
                }
            },
            Stmt::Expr(e) => {
                match e {
                    Expr::Call { func, args } => {
                        if printf_shape(args) {
                        } else if is_ext(func) && is_unset_cmd(args) {
                        } else if is_ext(func) && is_read_cmd(args) {
                        } else if writes_e(e, n) {
                            coverage_e(e, n);
                        }
                    },
                    _ => {
                        if writes_e(e, n) {
                            coverage_e(e, n);
                        }
                    },
                }
            },
            Stmt::Output(e) | Stmt::Die(e) | Stmt::Warn(e) | Stmt::SetChildError(e) => {
                if writes_e(e, n) {
                    coverage_e(e, n);
                }
            },
            Stmt::Return(e) | Stmt::Exit(e) => {
                if opt_w(e, n) {
                    match e {
                        Some(x) => coverage_e(x, n),
                        None => {},
                    }
                }
            },
            Stmt::WriteFile { path, content } => {
                if writes_e(path, n) {
                    coverage_e(path, n);
                }
                if writes_e(content, n) {
                    coverage_e(content, n);
                }
            },
            Stmt::Exec { cmd, args, redirects, env } => {
                if writes_e(cmd, n) {
                    coverage_e(cmd, n);
                }
                if writes_x(args, n) {
                    let i = choose|i: int| 0 <= i < args.len() && writes_e(args[i], n);
                    coverage_e(args[i], n);
                }
                if writes_x(redirects, n) {
                    let i = choose|i: int| 0 <= i < redirects.len() && writes_e(redirects[i], n);
                    coverage_e(redirects[i], n);
                }
                if writes_x(env, n) {
                    let i = choose|i: int| 0 <= i < env.len() && writes_e(env[i], n);
                    coverage_e(env[i], n);
                }
            },
            Stmt::Redirect { inner, targets } => {
                if writes_seq(inner, n) {
                    let i = choose|i: int| 0 <= i < inner.len() && writes_s(inner[i], n);
                    coverage_s(inner[i], n);
                }
                if writes_x(targets, n) {
                    let i = choose|i: int| 0 <= i < targets.len() && writes_e(targets[i], n);
                    coverage_e(targets[i], n);
                }
            },
            Stmt::Pipeline { stages } => {
                if exists|i: int| 0 <= i < stages.len() && writes_seq(stages[i], n) {
                    let i = choose|i: int|
                        0 <= i < stages.len() && writes_seq(stages[i], n);
                    if writes_seq(stages[i], n) {
                        let j = choose|j: int|
                            0 <= j < stages[i].len() && writes_s(stages[i][j], n);
                        coverage_s(stages[i][j], n);
                    }
                }
            },
            Stmt::If { cond, then, elsifs, else_ } => {
                if writes_e(cond, n) {
                    coverage_e(cond, n);
                }
                if writes_seq(then, n) {
                    let i = choose|i: int| 0 <= i < then.len() && writes_s(then[i], n);
                    coverage_s(then[i], n);
                }
                if exists|i: int|
                    0 <= i < elsifs.len()
                        && (writes_e(elsifs[i].0, n) || writes_seq(elsifs[i].1, n)) {
                    let i = choose|i: int|
                        0 <= i < elsifs.len()
                            && (writes_e(elsifs[i].0, n) || writes_seq(elsifs[i].1, n));
                    if writes_e(elsifs[i].0, n) {
                        coverage_e(elsifs[i].0, n);
                    }
                    if writes_seq(elsifs[i].1, n) {
                        let j = choose|j: int|
                            0 <= j < elsifs[i].1.len() && writes_s(elsifs[i].1[j], n);
                        coverage_s(elsifs[i].1[j], n);
                    }
                }
                if writes_seq(else_, n) {
                    let i = choose|i: int| 0 <= i < else_.len() && writes_s(else_[i], n);
                    coverage_s(else_[i], n);
                }
            },
            Stmt::Case { discriminant, bodies } => {
                if writes_e(discriminant, n) {
                    coverage_e(discriminant, n);
                }
                if exists|i: int| 0 <= i < bodies.len() && writes_seq(bodies[i], n) {
                    let i = choose|i: int|
                        0 <= i < bodies.len() && writes_seq(bodies[i], n);
                    if writes_seq(bodies[i], n) {
                        let j = choose|j: int|
                            0 <= j < bodies[i].len() && writes_s(bodies[i][j], n);
                        coverage_s(bodies[i][j], n);
                    }
                }
            },
            Stmt::Select { clauses } => {
                if exists|i: int|
                    0 <= i < clauses.len()
                        && (opt_w(clauses[i].ch, n) || opt_w(clauses[i].value, n)
                            || writes_seq(clauses[i].body, n)) {
                    let i = choose|i: int|
                        0 <= i < clauses.len()
                            && (opt_w(clauses[i].ch, n) || opt_w(clauses[i].value, n)
                                || writes_seq(clauses[i].body, n));
                    if opt_w(clauses[i].ch, n) {
                        match clauses[i].ch {
                            Some(x) => coverage_e(x, n),
                            None => {},
                        }
                    }
                    if opt_w(clauses[i].value, n) {
                        match clauses[i].value {
                            Some(x) => coverage_e(x, n),
                            None => {},
                        }
                    }
                    if writes_seq(clauses[i].body, n) {
                        let j = choose|j: int|
                            0 <= j < clauses[i].body.len() && writes_s(clauses[i].body[j], n);
                        coverage_s(clauses[i].body[j], n);
                    }
                }
            },
            Stmt::While { cond, body } | Stmt::DoWhile { cond, body } => {
                if writes_e(cond, n) {
                    coverage_e(cond, n);
                }
                if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                }
            },
            Stmt::For { var, iter, body } => {
                if n == var {
                } else if writes_e(iter, n) {
                    coverage_e(iter, n);
                } else if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                }
            },
            Stmt::ForInit { init, cond, step, body } => {
                if writes_seq(init, n) {
                    let i = choose|i: int| 0 <= i < init.len() && writes_s(init[i], n);
                    coverage_s(init[i], n);
                }
                if writes_e(cond, n) {
                    coverage_e(cond, n);
                }
                if writes_seq(step, n) {
                    let i = choose|i: int| 0 <= i < step.len() && writes_s(step[i], n);
                    coverage_s(step[i], n);
                }
                if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                }
            },
            Stmt::Function { body } => {
                if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                }
            },
            Stmt::Try { body, excepts, else_body, finally_body } => {
                if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                }
                if exists|i: int|
                    0 <= i < excepts.len()
                        && (opt_w(excepts[i].match_expr, n) || writes_seq(excepts[i].body, n)) {
                    let i = choose|i: int|
                        0 <= i < excepts.len()
                            && (opt_w(excepts[i].match_expr, n) || writes_seq(excepts[i].body, n));
                    if opt_w(excepts[i].match_expr, n) {
                        match excepts[i].match_expr {
                            Some(x) => coverage_e(x, n),
                            None => {},
                        }
                    }
                    if writes_seq(excepts[i].body, n) {
                        let j = choose|j: int|
                            0 <= j < excepts[i].body.len()
                                && writes_s(excepts[i].body[j], n);
                        coverage_s(excepts[i].body[j], n);
                    }
                }
                if writes_seq(else_body, n) {
                    let i = choose|i: int| 0 <= i < else_body.len() && writes_s(else_body[i], n);
                    coverage_s(else_body[i], n);
                }
                if writes_seq(finally_body, n) {
                    let i = choose|i: int|
                        0 <= i < finally_body.len() && writes_s(finally_body[i], n);
                    coverage_s(finally_body[i], n);
                }
            },
            Stmt::Scope(b) => {
                if writes_seq(b, n) {
                    let i = choose|i: int| 0 <= i < b.len() && writes_s(b[i], n);
                    coverage_s(b[i], n);
                }
            },
            Stmt::ExtTakeField { x, f } => {
                if n == x || n == f {
                }
            },
        }
    }
}

// COVERAGE (2/4 — statement sequences).
proof fn coverage_seq(b: Seq<Stmt>, n: Name)
    ensures writes_seq(b, n) ==> col_seq(b, n, Ch::Hoist),
    decreases b,
{
    if writes_seq(b, n) {
        let i = choose|i: int| 0 <= i < b.len() && writes_s(b[i], n);
        coverage_s(b[i], n);
    }
}

// COVERAGE (3/4 — expressions).
proof fn coverage_e(e: Expr, n: Name)
    ensures writes_e(e, n) ==> col_e(e, n, Ch::Hoist),
    decreases e,
{
    if writes_e(e, n) {
        match e {
            Expr::Lit | Expr::Var(_) => {
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
            Expr::Str { .. } | Expr::Arith { .. } => {
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
            Expr::Bin { l, r } => {
                assert(e == Expr::Bin { l, r });
                assert(writes_e(e, n) == (writes_e(*l, n) || writes_e(*r, n)));
                assert(col_e(e, n, Ch::Hoist) == (col_e(*l, n, Ch::Hoist) || col_e(*r, n, Ch::Hoist)));
                if writes_e(*l, n) {
                    coverage_e(*l, n);
                }
                if writes_e(*r, n) {
                    coverage_e(*r, n);
                }
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
            Expr::Interp { parts } | Expr::Arr { items: parts } => {
                assert(writes_e(e, n) == writes_x(parts, n));
                assert(col_e(e, n, Ch::Hoist) == col_x(parts, n, Ch::Hoist));
                if writes_x(parts, n) {
                    let i = choose|i: int| 0 <= i < parts.len() && writes_e(parts[i], n);
                    coverage_e(parts[i], n);
                    assert(col_x(parts, n, Ch::Hoist));
                }
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
            Expr::Arrow { body } => {
                assert(e == Expr::Arrow { body });
                assert(writes_e(e, n) == writes_seq(body, n));
                assert(col_e(e, n, Ch::Hoist) == col_seq(body, n, Ch::Hoist));
                if writes_seq(body, n) {
                    let i = choose|i: int| 0 <= i < body.len() && writes_s(body[i], n);
                    coverage_s(body[i], n);
                    assert(col_seq(body, n, Ch::Hoist));
                }
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
            Expr::Call { func, args } => {
                assert(e == Expr::Call { func, args });
                assert(writes_e(e, n) == (if is_ext(func) && is_declare_cmd(args) {
                    word_scan(args, WScan::Decl, n)
                } else if is_ext(func) && is_unset_cmd(args) {
                    word_scan(args, WScan::Unset, n)
                } else if is_ext(func) && printf_shape(args) {
                    printf_v_target(args) == Some(n) || writes_x(args, n)
                } else if is_ext(func) && let_shape(args) {
                    let_target(args) == Some(n) || writes_x(args, n)
                } else if func == c_param() {
                    param_gate(args) == Some(n) || writes_x(args, n)
                } else {
                    writes_x(args, n)
                }));
                assert(col_e(e, n, Ch::Hoist) == (if is_ext(func) && is_declare_cmd(args) {
                    word_scan(args, WScan::Decl, n)
                } else if is_ext(func) && is_unset_cmd(args) {
                    word_scan(args, WScan::Unset, n)
                } else if is_ext(func) && printf_shape(args) {
                    printf_v_target(args) == Some(n) || col_x(args, n, Ch::Hoist)
                } else if is_ext(func) && let_shape(args) {
                    let_target(args) == Some(n) || col_x(args, n, Ch::Hoist)
                } else if func == c_param() {
                    param_gate(args) == Some(n) || col_x(args, n, Ch::Hoist)
                } else {
                    col_x(args, n, Ch::Hoist)
                }));
                if is_ext(func) && is_declare_cmd(args) {
                    // word targets: the HOIST census's declare arm.
                } else if is_ext(func) && is_unset_cmd(args) {
                    // word targets: hoist's unset arm (stmt + expr).
                } else if is_ext(func) && printf_shape(args) {
                    if printf_v_target(args) == Some(n) {
                    } else if writes_x(args, n) {
                        let i = choose|i: int|
                            0 <= i < args.len() && writes_e(args[i], n);
                        coverage_e(args[i], n);
                        assert(col_x(args, n, Ch::Hoist));
                    }
                } else if is_ext(func) && let_shape(args) {
                    if let_target(args) == Some(n) {
                    } else if writes_x(args, n) {
                        let i = choose|i: int|
                            0 <= i < args.len() && writes_e(args[i], n);
                        coverage_e(args[i], n);
                        assert(col_x(args, n, Ch::Hoist));
                    }
                } else if func == c_param() {
                    if param_gate(args) == Some(n) {
                    } else if writes_x(args, n) {
                        let i = choose|i: int|
                            0 <= i < args.len() && writes_e(args[i], n);
                        coverage_e(args[i], n);
                        assert(col_x(args, n, Ch::Hoist));
                    }
                } else if writes_x(args, n) {
                    let i = choose|i: int| 0 <= i < args.len() && writes_e(args[i], n);
                    coverage_e(args[i], n);
                    assert(col_x(args, n, Ch::Hoist));
                }
                assert(writes_e(e, n) ==> col_e(e, n, Ch::Hoist));
            },
        }
    }
}

// COVERAGE (4/4 — expression sequences).
proof fn coverage_x(a: Seq<Expr>, n: Name)
    ensures writes_x(a, n) ==> col_x(a, n, Ch::Hoist),
    decreases a,
{
    if writes_x(a, n) {
        let i = choose|i: int| 0 <= i < a.len() && writes_e(a[i], n);
        coverage_e(a[i], n);
    }
}

// VETO EXACTNESS (1/4 — statements): the const-lift veto's set is
// exactly the param/raw write positions — sound AND precise.
proof fn veto_exact_s(s: Stmt, n: Name)
    ensures col_s(s, n, Ch::Veto) <==> paramw_s(s, n),
    decreases s,
{
    match s {
        Stmt::Assign { expr, .. } | Stmt::Declare { init: Some(expr), .. } => {
            veto_exact_e(expr, n);
        },
        Stmt::Declare { init: None, .. } => {},
        Stmt::Expr(e) | Stmt::Output(e) | Stmt::Die(e) | Stmt::Warn(e)
        | Stmt::SetChildError(e) => {
            veto_exact_e(e, n);
        },
        Stmt::Return(e) | Stmt::Exit(e) => {
            match e {
                Some(x) => veto_exact_e(x, n),
                None => {},
            }
        },
        Stmt::WriteFile { path, content } => {
            veto_exact_e(path, n);
            veto_exact_e(content, n);
        },
        Stmt::Exec { cmd, args, redirects, env } => {
            veto_exact_e(cmd, n);
            veto_exact_x(args, n);
            veto_exact_x(redirects, n);
            veto_exact_x(env, n);
        },
        Stmt::DeclareArray { elements } => veto_exact_x(elements, n),
        Stmt::Redirect { inner, targets } => {
            veto_exact_seq(inner, n);
            veto_exact_x(targets, n);
        },
        Stmt::Pipeline { stages } => {
            if col_s(s, n, Ch::Veto) {
                let i = choose|i: int|
                    0 <= i < stages.len() && col_seq(stages[i], n, Ch::Veto);
                if col_seq(stages[i], n, Ch::Veto) {
                    let j = choose|j: int|
                        0 <= j < stages[i].len() && col_s(stages[i][j], n, Ch::Veto);
                    veto_exact_s(stages[i][j], n);
                }
            }
            if paramw_s(s, n) {
                let i = choose|i: int|
                    0 <= i < stages.len() && paramw_seq(stages[i], n);
                if paramw_seq(stages[i], n) {
                    let j = choose|j: int|
                        0 <= j < stages[i].len() && paramw_s(stages[i][j], n);
                    veto_exact_s(stages[i][j], n);
                }
            }
        },
        Stmt::If { cond, then, elsifs, else_ } => {
            veto_exact_e(cond, n);
            veto_exact_seq(then, n);
            veto_exact_seq(else_, n);
            if col_s(s, n, Ch::Veto) || paramw_s(s, n) {
                if exists|i: int|
                    0 <= i < elsifs.len()
                        && (col_e(elsifs[i].0, n, Ch::Veto)
                            || col_seq(elsifs[i].1, n, Ch::Veto)) {
                    let i = choose|i: int|
                        0 <= i < elsifs.len()
                            && (col_e(elsifs[i].0, n, Ch::Veto)
                                || col_seq(elsifs[i].1, n, Ch::Veto));
                    if col_e(elsifs[i].0, n, Ch::Veto) {
                        veto_exact_e(elsifs[i].0, n);
                    }
                    if col_seq(elsifs[i].1, n, Ch::Veto) {
                        let j = choose|j: int|
                            0 <= j < elsifs[i].1.len()
                                && col_s(elsifs[i].1[j], n, Ch::Veto);
                        veto_exact_s(elsifs[i].1[j], n);
                    }
                }
                if exists|i: int|
                    0 <= i < elsifs.len()
                        && (paramw_e(elsifs[i].0, n) || paramw_seq(elsifs[i].1, n)) {
                    let i = choose|i: int|
                        0 <= i < elsifs.len()
                            && (paramw_e(elsifs[i].0, n) || paramw_seq(elsifs[i].1, n));
                    if paramw_e(elsifs[i].0, n) {
                        veto_exact_e(elsifs[i].0, n);
                    }
                    if paramw_seq(elsifs[i].1, n) {
                        let j = choose|j: int|
                            0 <= j < elsifs[i].1.len() && paramw_s(elsifs[i].1[j], n);
                        veto_exact_s(elsifs[i].1[j], n);
                    }
                }
            }
        },
        Stmt::Case { discriminant, bodies } => {
            veto_exact_e(discriminant, n);
            veto_exact_bodies(bodies, n);
        },
        Stmt::Select { clauses } => {
            if col_s(s, n, Ch::Veto) {
                let i = choose|i: int|
                    0 <= i < clauses.len()
                        && (opt_c(clauses[i].ch, n, Ch::Veto)
                            || opt_c(clauses[i].value, n, Ch::Veto)
                            || col_seq(clauses[i].body, n, Ch::Veto));
                if opt_c(clauses[i].ch, n, Ch::Veto) {
                    match clauses[i].ch {
                        Some(x) => veto_exact_e(x, n),
                        None => {},
                    }
                }
                if opt_c(clauses[i].value, n, Ch::Veto) {
                    match clauses[i].value {
                        Some(x) => veto_exact_e(x, n),
                        None => {},
                    }
                }
                if col_seq(clauses[i].body, n, Ch::Veto) {
                    let j = choose|j: int|
                        0 <= j < clauses[i].body.len()
                            && col_s(clauses[i].body[j], n, Ch::Veto);
                    veto_exact_s(clauses[i].body[j], n);
                }
            }
            if paramw_s(s, n) {
                let i = choose|i: int|
                    0 <= i < clauses.len()
                        && (paramw_opt(clauses[i].ch, n) || paramw_opt(clauses[i].value, n)
                            || paramw_seq(clauses[i].body, n));
                if paramw_opt(clauses[i].ch, n) {
                    match clauses[i].ch {
                        Some(x) => veto_exact_e(x, n),
                        None => {},
                    }
                }
                if paramw_opt(clauses[i].value, n) {
                    match clauses[i].value {
                        Some(x) => veto_exact_e(x, n),
                        None => {},
                    }
                }
                if paramw_seq(clauses[i].body, n) {
                    let j = choose|j: int|
                        0 <= j < clauses[i].body.len() && paramw_s(clauses[i].body[j], n);
                    veto_exact_s(clauses[i].body[j], n);
                }
            }
        },
        Stmt::While { cond, body } | Stmt::DoWhile { cond, body } => {
            veto_exact_e(cond, n);
            veto_exact_seq(body, n);
        },
        Stmt::For { iter, body, .. } => {
            veto_exact_e(iter, n);
            veto_exact_seq(body, n);
        },
        Stmt::ForInit { init, cond, step, body } => {
            veto_exact_seq(init, n);
            veto_exact_e(cond, n);
            veto_exact_seq(step, n);
            veto_exact_seq(body, n);
        },
        Stmt::Function { body } | Stmt::Scope(body) => veto_exact_seq(body, n),
        Stmt::Try { body, excepts, else_body, finally_body } => {
            veto_exact_seq(body, n);
            veto_exact_seq(else_body, n);
            veto_exact_seq(finally_body, n);
            if col_s(s, n, Ch::Veto) || paramw_s(s, n) {
                if exists|i: int|
                    0 <= i < excepts.len()
                        && (opt_c(excepts[i].match_expr, n, Ch::Veto)
                            || col_seq(excepts[i].body, n, Ch::Veto)) {
                    let i = choose|i: int|
                        0 <= i < excepts.len()
                            && (opt_c(excepts[i].match_expr, n, Ch::Veto)
                                || col_seq(excepts[i].body, n, Ch::Veto));
                    if opt_c(excepts[i].match_expr, n, Ch::Veto) {
                        match excepts[i].match_expr {
                            Some(x) => veto_exact_e(x, n),
                            None => {},
                        }
                    }
                    if col_seq(excepts[i].body, n, Ch::Veto) {
                        let j = choose|j: int|
                            0 <= j < excepts[i].body.len()
                                && col_s(excepts[i].body[j], n, Ch::Veto);
                        veto_exact_s(excepts[i].body[j], n);
                    }
                }
                if exists|i: int|
                    0 <= i < excepts.len()
                        && (paramw_opt(excepts[i].match_expr, n)
                            || paramw_seq(excepts[i].body, n)) {
                    let i = choose|i: int|
                        0 <= i < excepts.len()
                            && (paramw_opt(excepts[i].match_expr, n)
                                || paramw_seq(excepts[i].body, n));
                    if paramw_opt(excepts[i].match_expr, n) {
                        match excepts[i].match_expr {
                            Some(x) => veto_exact_e(x, n),
                            None => {},
                        }
                    }
                    if paramw_seq(excepts[i].body, n) {
                        let j = choose|j: int|
                            0 <= j < excepts[i].body.len()
                                && paramw_s(excepts[i].body[j], n);
                        veto_exact_s(excepts[i].body[j], n);
                    }
                }
            }
        },
        Stmt::ExtTakeField { .. } => {},
    }
}

// Per-clause-container helper for the Case arm: the two-hop access
// `bodies[i][j]` from a Stmt cannot be a termination measure step, so
// the walk is one hop at a time (s → bodies field → elements → seq).
proof fn veto_exact_bodies(bs: Seq<Seq<Stmt>>, n: Name)
    ensures
        (exists|i: int| 0 <= i < bs.len() && col_seq(bs[i], n, Ch::Veto))
            <==> (exists|i: int| 0 <= i < bs.len() && paramw_seq(bs[i], n)),
    decreases bs,
{
    if exists|i: int| 0 <= i < bs.len() && col_seq(bs[i], n, Ch::Veto) {
        let i = choose|i: int| 0 <= i < bs.len() && col_seq(bs[i], n, Ch::Veto);
        veto_exact_seq(bs[i], n);
    }
    if exists|i: int| 0 <= i < bs.len() && paramw_seq(bs[i], n) {
        let i = choose|i: int| 0 <= i < bs.len() && paramw_seq(bs[i], n);
        veto_exact_seq(bs[i], n);
    }
}

// VETO EXACTNESS (2/4).
proof fn veto_exact_seq(b: Seq<Stmt>, n: Name)
    ensures col_seq(b, n, Ch::Veto) <==> paramw_seq(b, n),
    decreases b,
{
    if col_seq(b, n, Ch::Veto) {
        let i = choose|i: int| 0 <= i < b.len() && col_s(b[i], n, Ch::Veto);
        veto_exact_s(b[i], n);
    }
    if paramw_seq(b, n) {
        let i = choose|i: int| 0 <= i < b.len() && paramw_s(b[i], n);
        veto_exact_s(b[i], n);
    }
}

// VETO EXACTNESS (3/4 — expressions).
proof fn veto_exact_e(e: Expr, n: Name)
    ensures col_e(e, n, Ch::Veto) <==> paramw_e(e, n),
    decreases e,
{
    match e {
        Expr::Lit | Expr::Var(_) | Expr::Arith { .. } => {},
        Expr::Str { .. } => {},
        Expr::Bin { l, r } => {
            veto_exact_e(*l, n);
            veto_exact_e(*r, n);
        },
        Expr::Interp { parts } | Expr::Arr { items: parts } => veto_exact_x(parts, n),
        Expr::Arrow { body } => veto_exact_seq(body, n),
        Expr::Call { func, args } => {
            if func == c_param() {
                // both sides are `gate ∨ arg-walk`; the <==> on the
                // arg walk (one hop: field) transports it directly.
                veto_exact_x(args, n);
            } else {
                // one hop: the non-param chain is the arg walk on both
                // sides — the <==> transports through it directly.
                veto_exact_x(args, n);
            }
        },
    }
}

// VETO EXACTNESS (4/4).
proof fn veto_exact_x(a: Seq<Expr>, n: Name)
    ensures col_x(a, n, Ch::Veto) <==> paramw_x(a, n),
    decreases a,
{
    if col_x(a, n, Ch::Veto) {
        let i = choose|i: int| 0 <= i < a.len() && col_e(a[i], n, Ch::Veto);
        veto_exact_e(a[i], n);
    }
    if paramw_x(a, n) {
        let i = choose|i: int| 0 <= i < a.len() && paramw_e(a[i], n);
        veto_exact_e(a[i], n);
    }
}

// ── THE EV BRIDGE (collector gate ⇔ denotational write) ─────────────

// The gate's target is a plain identifier (the source's element guard:
// `arr[i]` names are neutralised before the write arm dispatches).
proof fn param_gate_sound(a: Seq<Expr>, n: Name)
    requires param_gate(a) == Some(n),
    ensures is_ident(n),
{
}

// Locality: a gated param evaluation changes the store at its target
// and NOWHERE else.
proof fn ev_locality(a: Seq<Expr>, s0: Store, m: Name)
    ensures
        match param_gate(a) {
            Some(n) => m != n ==> lookup(ev_param_store(a, s0), m) == lookup(s0, m),
            None => true,
        },
{
    if param_gate(a).is_some() {
        let n = param_gate(a).unwrap();
        if m != n {
            assert(lookup(s0.insert(n, Some(Seq::empty())), m) == lookup(s0, m));
        }
    }
}

// No gate ⇒ no write: a read-param (or a non-ident name) leaves the
// store exactly as it was.
proof fn ev_readonly(a: Seq<Expr>, s0: Store)
    requires param_gate(a).is_none(),
    ensures ev_param_store(a, s0) == s0,
{
}

// Completeness: whenever the gate holds, there IS an initial store the
// evaluation changes — the write the collectors' target names.
proof fn ev_gate_writes(a: Seq<Expr>)
    requires param_gate(a).is_some(),
    ensures ev_param_store(a, Map::empty()) != Map::empty(),
{
    assert(param_gate(a).unwrap() == param_gate(a).unwrap());
    assert(lookup(Map::empty(), param_gate(a).unwrap()) == Option::None);
}

// ── FIDELITY PINS (the 2026-10-28 fixes, asserted both ways) ────────

pub open spec fn param_call(op: Str, name: Str, word: Str) -> Expr {
    Expr::Call {
        func: c_param(),
        args: seq![Expr::Str { text: op, single: false }, Expr::Str { text: name, single: false },
            Expr::Str { text: word, single: false }],
    }
}

pub open spec fn exec_call(args: Seq<Expr>) -> Expr {
    Expr::Call { func: c_exec(), args }
}

// t04/v10: the writing param arm — every walker collects it at the
// expr level (the shape that produced `y` undeclared at cc).
proof fn pin_t04_param()
    ensures
        col_s(Stmt::Expr(param_call(seq![':', '='], seq!['y'], seq!['D'])), seq!['y'],
            Ch::Inv),
        col_s(Stmt::Expr(param_call(seq![':', '='], seq!['y'], seq!['D'])), seq!['y'],
            Ch::Hoist),
        col_s(Stmt::Expr(param_call(seq![':', '='], seq!['y'], seq!['D'])), seq!['y'],
            Ch::Veto),
        paramw_s(Stmt::Expr(param_call(seq![':', '='], seq!['y'], seq!['D'])), seq!['y']),
        writes_s(Stmt::Expr(param_call(seq![':', '='], seq!['y'], seq!['D'])), seq!['y']),
{
    let e = param_call(seq![':', '='], seq!['y'], seq!['D']);
    assert(param_gate(match e { Expr::Call { args, .. } => args, _ => Seq::empty() })
        == Some(seq!['y']));
}

// p13: a write in a WHILE COND is walked by both decl censuses —
// removing either arm re-breaks `while [ -n "${y:=Z}" ]`.
proof fn pin_while_cond()
    ensures
        writes_s(Stmt::While { cond: param_call(seq![':', '='], seq!['y'], seq!['Z']),
            body: Seq::empty() }, seq!['y']),
        col_s(Stmt::While { cond: param_call(seq![':', '='], seq!['y'], seq!['Z']),
            body: Seq::empty() }, seq!['y'], Ch::Inv),
        col_s(Stmt::While { cond: param_call(seq![':', '='], seq!['y'], seq!['Z']),
            body: Seq::empty() }, seq!['y'], Ch::Hoist),
{
    let st = Stmt::While { cond: param_call(seq![':', '='], seq!['y'], seq!['Z']),
        body: Seq::empty() };
    assert(writes_s(st, seq!['y']));
    match st {
        Stmt::While { cond, .. } => assert(writes_e(cond, seq!['y'])),
        _ => {},
    }
}

// v10b: `${z:=K}` arriving as ONE raw Str is read out of the text by
// all three walks (and is a write).
proof fn pin_raw_nested()
    ensures
        writes_e(Expr::Str { text: seq!['$', '{', 'z', ':', '=', 'K', '}'], single: false },
            seq!['z']),
        col_e(Expr::Str { text: seq!['$', '{', 'z', ':', '=', 'K', '}'], single: false },
            seq!['z'], Ch::Inv),
        col_e(Expr::Str { text: seq!['$', '{', 'z', ':', '=', 'K', '}'], single: false },
            seq!['z'], Ch::Hoist),
        col_e(Expr::Str { text: seq!['$', '{', 'z', ':', '=', 'K', '}'], single: false },
            seq!['z'], Ch::Veto),
        // fidelity both ways: a SINGLE-QUOTED raw string never expands.
        !col_e(Expr::Str { text: seq!['$', '{', 'z', ':', '=', 'K', '}'], single: true },
            seq!['z'], Ch::Veto),
{
    let t = seq!['$', '{', 'z', ':', '=', 'K', '}'];
    assert(t.subrange(2, 3) == seq!['z']);
    assert(t.subrange(3, 5) == seq![':', '=']);
    assert(ident_run(t.subrange(3, t.len() as int)) == 0) by {
        reveal_with_fuel(ident_run, 4);
    };
    assert(ident_run(t.subrange(2, t.len() as int)) == 1) by {
        reveal_with_fuel(ident_run, 4);
    };
    assert(raw_hit(t, seq!['z'], 0)) by {
        reveal_with_fuel(ident_run, 4);
    };
}

// fn3: `echo "${y:=K}"` at the end of a function lowers to
// Return(param(…)) — hoist collects it, inventory deliberately has NO
// Return arm (fn-local routing is hoist's job).
proof fn pin_return_fn3()
    ensures
        writes_s(Stmt::Return(Some(param_call(seq![':', '='], seq!['y'], seq!['K']))),
            seq!['y']),
        col_s(Stmt::Return(Some(param_call(seq![':', '='], seq!['y'], seq!['K']))), seq!['y'],
            Ch::Hoist),
        !col_s(Stmt::Return(Some(param_call(seq![':', '='], seq!['y'], seq!['K']))), seq!['y'],
            Ch::Inv),
{
    let st = Stmt::Return(Some(param_call(seq![':', '='], seq!['y'], seq!['K'])));
    assert(opt_w(Some(param_call(seq![':', '='], seq!['y'], seq!['K'])), seq!['y']));
    assert(opt_c(Some(param_call(seq![':', '='], seq!['y'], seq!['K'])), seq!['y'],
        Ch::Hoist));
    assert(writes_s(st, seq!['y']));
    assert(col_s(st, seq!['y'], Ch::Hoist));
    assert(!col_s(st, seq!['y'], Ch::Inv));
}

// fix (a): a write in an ELSIF COND is walked by ALL THREE (the hoist
// arm is this model's fix; removing it kills coverage inside fns).
proof fn pin_elsif_cond()
    ensures
        writes_s(Stmt::If { cond: Expr::Lit, then: Seq::empty(),
            elsifs: seq![(param_call(seq![':', '='], seq!['y'], seq!['A']), Seq::empty())],
            else_: Seq::empty() }, seq!['y']),
        col_s(Stmt::If { cond: Expr::Lit, then: Seq::empty(),
            elsifs: seq![(param_call(seq![':', '='], seq!['y'], seq!['A']), Seq::empty())],
            else_: Seq::empty() }, seq!['y'], Ch::Inv),
        col_s(Stmt::If { cond: Expr::Lit, then: Seq::empty(),
            elsifs: seq![(param_call(seq![':', '='], seq!['y'], seq!['A']), Seq::empty())],
            else_: Seq::empty() }, seq!['y'], Ch::Hoist),
        col_s(Stmt::If { cond: Expr::Lit, then: Seq::empty(),
            elsifs: seq![(param_call(seq![':', '='], seq!['y'], seq!['A']), Seq::empty())],
            else_: Seq::empty() }, seq!['y'], Ch::Veto),
{
    let st = Stmt::If { cond: Expr::Lit, then: Seq::empty(),
        elsifs: seq![(param_call(seq![':', '='], seq!['y'], seq!['A']), Seq::empty())],
        else_: Seq::empty() };
    assert(writes_e(param_call(seq![':', '='], seq!['y'], seq!['A']), seq!['y']));
    match st {
        Stmt::If { elsifs, .. } => {
            assert(elsifs[0].0 == param_call(seq![':', '='], seq!['y'], seq!['A']));
            assert(0 <= 0int && 0int < elsifs.len());
            assert(exists|i: int| 0 <= i < elsifs.len()
                && (writes_e(elsifs[i].0, seq!['y']) || writes_seq(elsifs[i].1, seq!['y'])));
        },
        _ => {},
    }
    assert(writes_s(st, seq!['y']));
    assert(col_s(st, seq!['y'], Ch::Inv));
    assert(col_s(st, seq!['y'], Ch::Hoist));
    assert(col_s(st, seq!['y'], Ch::Veto));
}

// fix (c): expr-position `printf -v NAME` — hoist + inventory extract
// the target; the veto has NO printf arm (fidelity).
proof fn pin_printf_nested()
    ensures
        writes_e(exec_call(seq![Expr::Str { text: seq!['p', 'r', 'i', 'n', 't', 'f'],
                    single: false },
                Expr::Arr { items: seq![Expr::Str { text: seq!['-', 'v'], single: false },
                        Expr::Str { text: seq!['y'], single: false }] },
                Expr::Str { text: seq!['%', 's'], single: false }]), seq!['y']),
        col_e(exec_call(seq![Expr::Str { text: seq!['p', 'r', 'i', 'n', 't', 'f'],
                    single: false },
                Expr::Arr { items: seq![Expr::Str { text: seq!['-', 'v'], single: false },
                        Expr::Str { text: seq!['y'], single: false }] },
                Expr::Str { text: seq!['%', 's'], single: false }]), seq!['y'], Ch::Inv),
        col_e(exec_call(seq![Expr::Str { text: seq!['p', 'r', 'i', 'n', 't', 'f'],
                    single: false },
                Expr::Arr { items: seq![Expr::Str { text: seq!['-', 'v'], single: false },
                        Expr::Str { text: seq!['y'], single: false }] },
                Expr::Str { text: seq!['%', 's'], single: false }]), seq!['y'], Ch::Hoist),
        !col_e(exec_call(seq![Expr::Str { text: seq!['p', 'r', 'i', 'n', 't', 'f'],
                    single: false },
                Expr::Arr { items: seq![Expr::Str { text: seq!['-', 'v'], single: false },
                        Expr::Str { text: seq!['y'], single: false }] },
                Expr::Str { text: seq!['%', 's'], single: false }]), seq!['y'], Ch::Veto),
{
    let args = seq![Expr::Str { text: seq!['p', 'r', 'i', 'n', 't', 'f'], single: false },
        Expr::Arr { items: seq![Expr::Str { text: seq!['-', 'v'], single: false },
                Expr::Str { text: seq!['y'], single: false }] },
        Expr::Str { text: seq!['%', 's'], single: false }];
    let e = exec_call(args);
    assert(writes_e(e, seq!['y']));
    assert(col_e(e, seq!['y'], Ch::Inv));
    assert(col_e(e, seq!['y'], Ch::Hoist));
    // the veto has NO printf arm: every arg is a read (refute the
    // per-arg existential explicitly — concrete-seq witnesses).
    assert(!col_e(args[0], seq!['y'], Ch::Veto));
    assert(!col_e(args[2], seq!['y'], Ch::Veto));
    match args[1] {
        Expr::Arr { items } => {
            assert(!col_e(items[0], seq!['y'], Ch::Veto));
            assert(!col_e(items[1], seq!['y'], Ch::Veto));
        },
        _ => {},
    }
    match args[1] {
        Expr::Arr { items } => {
            assert(!col_e(items[0], seq!['y'], Ch::Veto));
            assert(!col_e(items[1], seq!['y'], Ch::Veto));
            if col_x(items, seq!['y'], Ch::Veto) {
                let w = choose|j: int|
                    0 <= j < items.len() && col_e(items[j], seq!['y'], Ch::Veto);
                assert(w == 0int || w == 1int);
                if w == 0int {
                    assert(false);
                }
                if w == 1int {
                    assert(false);
                }
            }
            assert(!col_e(args[1], seq!['y'], Ch::Veto));
        },
        _ => {},
    }
    if col_x(args, seq!['y'], Ch::Veto) {
        let w = choose|i: int|
            0 <= i < args.len() && col_e(args[i], seq!['y'], Ch::Veto);
        assert(w == 0int || w == 1int || w == 2int);
        if w == 0int {
            assert(false);
        }
        if w == 1int {
            assert(false);
        }
        if w == 2int {
            assert(false);
        }
    }
    assert(!col_e(e, seq!['y'], Ch::Veto));
}

// the element guard: an `arr[i]` name is not a plain identifier, so
// nobody collects it and the write gate refuses it (bash writes the
// ELEMENT; the neutralised node is read-only in C — documented).
proof fn pin_array_elem()
    ensures
        param_gate(seq![Expr::Str { text: seq![':', '='], single: false },
            Expr::Str { text: seq!['a', '[', '0', ']'], single: false },
            Expr::Str { text: seq!['v'], single: false }]) == Option::None,
        !paramw_e(param_call(seq![':', '='], seq!['a', '[', '0', ']'], seq!['v']), seq!['a']),
{
    let pc = param_call(seq![':', '='], seq!['a', '[', '0', ']'], seq!['v']);
    assert(param_gate(seq![Expr::Str { text: seq![':', '='], single: false },
        Expr::Str { text: seq!['a', '[', '0', ']'], single: false },
        Expr::Str { text: seq!['v'], single: false }]) == Option::None);
    match pc {
        Expr::Call { args, .. } => {
            assert(!paramw_e(args[0], seq!['a']));
            assert(!paramw_e(args[1], seq!['a']));
            assert(!paramw_e(args[2], seq!['a']));
            assert(forall|i: int| 0 <= i < args.len() ==> !paramw_e(args[i], seq!['a']));
            assert(!paramw_x(args, seq!['a']));
        },
        _ => {},
    }
    assert(!paramw_e(pc, seq!['a']));
}

// fix (2026-10-28): Select ch/value — hoist + veto walk them,
// inventory has no Select arm (fidelity both ways).
proof fn pin_select_ch()
    ensures
        writes_s(Stmt::Select { clauses: seq![Clause { ch: Some(param_call(seq![':', '='],
                        seq!['y'], seq!['Z'])), value: None, body: Seq::empty() }] },
            seq!['y']),
        col_s(Stmt::Select { clauses: seq![Clause { ch: Some(param_call(seq![':', '='],
                        seq!['y'], seq!['Z'])), value: None, body: Seq::empty() }] },
            seq!['y'], Ch::Hoist),
        col_s(Stmt::Select { clauses: seq![Clause { ch: Some(param_call(seq![':', '='],
                        seq!['y'], seq!['Z'])), value: None, body: Seq::empty() }] },
            seq!['y'], Ch::Veto),
        !col_s(Stmt::Select { clauses: seq![Clause { ch: Some(param_call(seq![':', '='],
                        seq!['y'], seq!['Z'])), value: None, body: Seq::empty() }] },
            seq!['y'], Ch::Inv),
{
    let st = Stmt::Select { clauses: seq![Clause { ch: Some(param_call(seq![':', '='],
            seq!['y'], seq!['Z'])), value: None, body: Seq::empty() }] };
    let chp = param_call(seq![':', '='], seq!['y'], seq!['Z']);
    assert(writes_e(chp, seq!['y']));
    assert(col_e(chp, seq!['y'], Ch::Hoist));
    assert(col_e(chp, seq!['y'], Ch::Veto));
    match st {
        Stmt::Select { clauses } => {
            assert(clauses[0].ch == Some(param_call(seq![':', '='], seq!['y'], seq!['Z'])));
            assert(0 <= 0int && 0int < clauses.len());
            assert(exists|i: int| 0 <= i < clauses.len()
                && (opt_w(clauses[i].ch, seq!['y']) || opt_w(clauses[i].value, seq!['y'])
                    || writes_seq(clauses[i].body, seq!['y'])));
        },
        _ => {},
    }
    assert(writes_s(st, seq!['y']));
    assert(col_s(st, seq!['y'], Ch::Hoist));
    assert(col_s(st, seq!['y'], Ch::Veto));
    assert(!col_s(st, seq!['y'], Ch::Inv));
}

} // verus!

fn main() {}
