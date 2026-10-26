//! str_chop_cursor — the O(n²) front-chop tokenizer → one O(1) cursor.
//!
//! The MIMEcroft-class hot loop pairs a head-chop with a self-chop:
//!
//! ```sh
//! while [ -n "$tup_s" ]; do
//!     m=${tup_s%%|*}; tup_s=${tup_s#*|}     # ×N pairs per iteration
//! done
//! ```
//!
//! Every `${s#*D}` rewrites `s` to its own tail, so the k-th pair
//! re-scans the first k−1 separators: N pairs over a W-byte payload
//! scan W²/2 bytes (the demo's ~6.8 GB of `strchr` traffic — the
//! measured #1 cost). Each `${s%%D*}` / `${s#*D}` pair is a FRONT-CHOP:
//! after the self-chop the remainder sits at a known offset of the
//! ORIGINAL buffer, so the whole series is one cursor.
//!
//! This transform replaces the series with a cursor var `{s}_fc` — the
//! ABSOLUTE byte offset of the unconsumed tail (byte 0 = start of the
//! string, so an unset/unchopped cursor reads ""/0 exactly like the
//! shell default) — plus two shared shir_nodes (shir_nodes/*.node,
//! PLAN §11 marketplace offers) rendered O(1) per chop by the C backend:
//!
//! ```text
//! x      = FieldGrab(s, off, D)    # head field of the tail at `off`
//! {s}_fc = FieldStrip(s, off, D)   # absolute offset past that D
//! {s}_fc = 0                       # one init before the first use
//! ```
//!
//! The C arms never rescan the whole string (a defensive strlen would
//! reinstate O(W) per chop — the demo's setup-phase AVX2 strlen burn):
//! they lean on read law 1 below for `off <= strlen(s)`, and
//! `cursor_init_survives_full_registry` pins that the init that law
//! needs survives the FULL transform registry, not just this
//! transform in isolation.
//!
//! ## Read law (the soundness contract — refuse > guess everywhere)
//!
//! 1. **Init window.** A full write of `s` (init) precedes every use in
//!    source order (address-downstream, not merely ordinal); no
//!    statement between init and the last use writes `s` — a mid-window
//!    write would move the buffer under the cursor.
//! 2. **Reads.** `s` is read only at/before init in init's own list,
//!    and never inside a loop that re-runs a use: a read after a chop
//!    sees the chopped value, which the cursor no longer reproduces
//!    (this is why a `[ -n "$c" ]` back-edge over a `c=${c#*X}`
//!    tokenizer REFUSES instead of corrupting — matches TakeCx::plain's
//!    window, but on `s` rather than only the extracted field).
//! 3. **Region.** Every statement from init to the last use — plus the
//!    full body of any loop containing a use — is closed under an
//!    allowlisted kind set and pure expressions, so nothing observable
//!    happens inside the window that the rewrite does not model.
//! 4. **Program-wide.** No unknown stmt/expr node (Ext stmts, RawExpr,
//!    Arrow/Lambda bodies — anything whose hidden fields could
//!    reference `s`), no function shadowing of `s` or a grab target,
//!    no numeric position (arith/index/array) for `s` or a grab target
//!    (the C render keeps them on the char* channel), no use/write/read
//!    inside a background (async) subtree.
//! 5. **Pairs.** At least one grab-shaped `x=${s%%D*}` (magic-free `D`,
//!    literal pattern — no `$`) and one self-strip `s=${s#*D}` convert;
//!    every such occurrence of `s` must convert or the candidate is
//!    refused (a half-converted series would mix chopped and cursor
//!    reads).
//!
//! Target-gated to `c` (transform_applies — split-in-place precedent:
//! only the C backend renders FieldGrab/FieldStrip today; a backend
//! accepts the offer by adding the two arms and its target to the
//! gate). `SH2_NO_STR_CHOP_CURSOR=1` disables it (A/B firewall for the
//! perf gates: render_ab.sh runs with the env var pinned both ways).

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{ArithAst, AssignTarget, IrCaseClause, IrExpr, IrStmt, StrStyle};
use crate::shir_nodes::{FieldGrab, FieldStrip, FieldStripEnd};

/// Env-var kill switch (A/B gates pin it both ways).
fn enabled() -> bool {
    if std::env::var_os("SH2_NO_STR_CHOP_CURSOR").is_some() {
        return false;
    }
    matches!(crate::transforms::target().as_deref(), Some("c"))
}

/// SH2_DBG_CHOP=1: log candidate/refusal reasons (the empirical loop
/// when a program you EXPECT to convert does not fire).
fn dbg() -> bool {
    std::env::var_os("SH2_DBG_CHOP").is_some()
}

/// A validated conversion plan: one cursor for one `s`.
struct Edits {
    s: String,
    off: String,
    /// addr → delim for every `${s%%D*}` head-chop.
    grabs: BTreeMap<Vec<usize>, String>,
    /// addr → delim for every `${s#*D}` self-chop.
    strips: BTreeMap<Vec<usize>, String>,
    /// addr of the full write of `s` the cursor inits after.
    init_addr: Vec<usize>,
    /// addr of every RECOGNIZED guarded front-chop `case`: pass 1
    /// collapses the whole statement to its chop arm (the default arm
    /// is what FieldStripEnd's drain encodes — it never survives).
    guards: BTreeSet<Vec<usize>>,
    /// addr of each self-chop living inside a guard: it advances with
    /// the DRAINING variant, because the dropped `*) x=$s; s=""` arm
    /// emptied `s` and a plain FieldStrip would replay the last field.
    drains: BTreeSet<Vec<usize>>,
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if !enabled() {
        return false;
    }
    let mut changed = false;
    loop {
        // Prescan (s = "" never matches) — collects the vars that ever
        // appear in a %%/# param call: the only cursor candidates.
        let mut pre = An::new("");
        pre.scan(stmts);
        let mut fired = false;
        for s in pre.param_vars.iter() {
            if let Some(ed) = plan(stmts, s) {
                apply(stmts, &ed);
                changed = true;
                fired = true;
                break;
            }
        }
        if !fired {
            break;
        }
    }
    changed
}

fn plan(stmts: &[IrStmt], s: &str) -> Option<Edits> {
    let mut an = An::new(s);
    an.scan(stmts);
    match an.validate() {
        Ok(ed) => {
            if dbg() {
                eprintln!(
                    "CHOP fire s={s} grabs={} strips={} off={}",
                    ed.grabs.len(),
                    ed.strips.len(),
                    ed.off
                );
            }
            Some(ed)
        }
        Err(why) => {
            if dbg() {
                eprintln!("CHOP refuse s={s}: {why}");
            }
            None
        }
    }
}

// ── scan state ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct Ev {
    /// `s` is read somewhere in this expression.
    mention: bool,
    /// External side effect (calls, capture, raw code, asm).
    impure: bool,
    /// Node we cannot see through — candidate refused outright.
    unknown: bool,
    /// `s` is WRITTEN somewhere inside (setVar/exec-form/arith-assign).
    wr: bool,
}

impl Ev {
    fn merge(&mut self, o: Ev) {
        self.mention |= o.mention;
        self.impure |= o.impure;
        self.unknown |= o.unknown;
        self.wr |= o.wr;
    }
}

/// Statement record: ordinal range + region-eligibility facts.
struct Rec {
    ord: usize,
    end: usize,
    /// Statement address (list-path + index) — the loop-prefix checks
    /// need it (ordinals alone cannot say "inside that loop").
    addr: Vec<usize>,
    /// Kind is closed, straight-line window material (Assign/if/loops/
    /// block/continue/break/return/exit).
    kind_ok: bool,
    /// Own expressions are pure.
    pure: bool,
    /// Hosts a front-chop use (self or subtree — drives loop region).
    has_use: bool,
    is_loop: bool,
    /// What the statement IS (SH2_DBG_CHOP: "region stmt ord N" alone
    /// sent me hunting through a 25k-line render to find one impure
    /// call).
    label: String,
}

enum UKind {
    Grab { target: String, delim: String },
    Strip { delim: String },
}

struct Use {
    addr: Vec<usize>,
    ord: usize,
    kind: UKind,
}

struct Wri {
    ord: usize,
    full: bool,
    addr: Vec<usize>,
}

struct Rd {
    ord: usize,
    addr: Vec<usize>,
}

struct An {
    s: String,
    recs: Vec<Rec>,
    uses: Vec<Use>,
    writes: Vec<Wri>,
    reads: Vec<Rd>,
    grab_targets: BTreeSet<String>,
    /// Var-ish names anywhere (collision-free cursor name).
    names: BTreeSet<String>,
    /// Vars passed to a %%/# param call (cursor candidates).
    param_vars: BTreeSet<String>,
    /// Numeric-position refs: arith / index keys / array names.
    arith_refs: BTreeSet<String>,
    idx_refs: BTreeSet<String>,
    array_vars: BTreeSet<String>,
    fn_names: BTreeSet<String>,
    fn_params: BTreeSet<String>,
    /// Background (async) statement addrs — no window part may live
    /// under one.
    bg_addrs: Vec<Vec<usize>>,
    /// Host addrs of Arrow/Lambda delayed bodies — walked for read/
    /// write visibility, but no window part may live strictly INSIDE
    /// one (the rewrite walk does not descend there).
    arrow_hosts: Vec<Vec<usize>>,
    /// Globally unique slot number per delayed body (never reset —
    /// structural slots are small and per-stmt, this namespace never
    /// collides with them or with itself).
    arrow_seq: usize,
    /// An unseeable node exists → refuse every candidate.
    unknown: bool,
    /// What the first unseeable node was (SH2_DBG_CHOP report).
    unknown_why: Option<String>,
    /// addr of each guarded front-chop `case` seen for this candidate.
    guards: BTreeSet<Vec<usize>>,
    next_ord: usize,
}

impl An {
    fn new(s: &str) -> Self {
        An {
            s: s.to_string(),
            recs: Vec::new(),
            uses: Vec::new(),
            writes: Vec::new(),
            reads: Vec::new(),
            grab_targets: BTreeSet::new(),
            names: BTreeSet::new(),
            param_vars: BTreeSet::new(),
            arith_refs: BTreeSet::new(),
            idx_refs: BTreeSet::new(),
            array_vars: BTreeSet::new(),
            fn_names: BTreeSet::new(),
            fn_params: BTreeSet::new(),
            bg_addrs: Vec::new(),
            arrow_hosts: Vec::new(),
            arrow_seq: 0,
            unknown: false,
            unknown_why: None,
            guards: BTreeSet::new(),
            next_ord: 0,
        }
    }

    fn mark_unknown(&mut self, why: &str) {
        self.unknown = true;
        if self.unknown_why.is_none() {
            self.unknown_why = Some(why.to_string());
        }
    }

    fn scan(&mut self, stmts: &[IrStmt]) {
        let mut addr = Vec::new();
        self.list(stmts, &mut addr, None);
    }

    /// Walk a child statement list. `slot` distinguishes sibling lists
    /// of one statement (then vs else etc.) so addresses are unambiguous.
    fn list(&mut self, stmts: &[IrStmt], addr: &mut Vec<usize>, slot: Option<usize>) -> bool {
        if let Some(sl) = slot {
            addr.push(sl);
        }
        let mut any = false;
        for (i, st) in stmts.iter().enumerate() {
            addr.push(i);
            let ord = self.next_ord;
            self.next_ord += 1;
            let ri = self.recs.len();
            self.recs.push(Rec {
                ord,
                end: ord,
                addr: addr.clone(),
                kind_ok: false,
                pure: true,
                has_use: false,
                is_loop: false,
                label: st_label(st),
            });
            let hu = self.stmt(st, ord, addr, ri);
            self.recs[ri].end = self.next_ord - 1;
            self.recs[ri].has_use = hu;
            any |= hu;
            addr.pop();
        }
        if slot.is_some() {
            addr.pop();
        }
        any
    }

    fn note_read(&mut self, ev: &Ev, ord: usize, addr: &[usize]) {
        if ev.mention {
            self.reads.push(Rd {
                ord,
                addr: addr.to_vec(),
            });
        }
    }

    fn stmt(&mut self, st: &IrStmt, ord: usize, addr: &mut Vec<usize>, ri: usize) -> bool {
        let mut hu = false;
        match st {
            IrStmt::Assign { targets, expr, asm } => {
                self.recs[ri].kind_ok = true;
                if asm.is_some() {
                    // Declaration-position asm label: not modelled.
                    self.mark_unknown("assign-asm-label");
                }
                let mut ev = self.expr(expr, addr);
                for t in targets {
                    self.names.insert(t.var.clone());
                    for ix in &t.indices {
                        ev.merge(self.expr(ix, addr));
                    }
                }
                let single = targets.len() == 1 && targets[0].indices.is_empty();
                // One source of truth: guard_shape() recognizes a guarded
                // front-chop with THIS classifier, so a guard can never be
                // recognized and then walked as a different pair.
                let classified: Option<UKind> = chop_kind(st, &self.s);
                match classified {
                    Some(k) => {
                        if let UKind::Grab { target, .. } = &k {
                            self.grab_targets.insert(target.clone());
                        }
                        self.uses.push(Use {
                            addr: addr.clone(),
                            ord,
                            kind: k,
                        });
                        hu = true;
                        if ev.unknown {
                            self.unknown = true;
                        }
                        self.recs[ri].pure = !ev.impure;
                    }
                    None => {
                        if single && targets[0].var == self.s {
                            self.writes.push(Wri {
                                ord,
                                full: true,
                                addr: addr.clone(),
                            });
                        } else if targets.iter().any(|t| t.var == self.s) {
                            self.writes.push(Wri {
                                ord,
                                full: false,
                                addr: addr.clone(),
                            });
                        }
                        if ev.wr {
                            self.writes.push(Wri {
                                ord,
                                full: false,
                                addr: addr.clone(),
                            });
                        }
                        self.note_read(&ev, ord, addr);
                        if ev.unknown {
                            self.unknown = true;
                        }
                        self.recs[ri].pure = !ev.impure;
                    }
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                let mut ev = Ev::default();
                if let Some(e) = init {
                    ev = self.expr(e, addr);
                }
                for d in vars {
                    self.names.insert(d.name.clone());
                    if d.name == self.s {
                        self.writes.push(Wri {
                            ord,
                            full: false,
                            addr: addr.clone(),
                        });
                    }
                }
                self.note_read(&ev, ord, addr);
                if ev.wr {
                    self.writes.push(Wri {
                        ord,
                        full: false,
                        addr: addr.clone(),
                    });
                }
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.names.insert(var.clone());
                self.array_vars.insert(var.clone());
                self.idx_refs.insert(var.clone());
                if var == &self.s {
                    self.writes.push(Wri {
                        ord,
                        full: false,
                        addr: addr.clone(),
                    });
                }
                let mut ev = Ev::default();
                for e in elements {
                    ev.merge(self.expr(e, addr));
                }
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                self.recs[ri].kind_ok = true;
                let mut ev = self.expr(cond, addr);
                hu |= self.list(then, addr, Some(0));
                for (k, (c, b)) in elsifs.iter().enumerate() {
                    ev.merge(self.expr(c, addr));
                    hu |= self.list(b, addr, Some(1 + k));
                }
                hu |= self.list(else_, addr, Some(1 + elsifs.len()));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::While { cond, body } => {
                self.recs[ri].kind_ok = true;
                self.recs[ri].is_loop = true;
                let ev = self.expr(cond, addr);
                hu |= self.list(body, addr, Some(0));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::DoWhile { body, cond, .. } => {
                self.recs[ri].kind_ok = true;
                self.recs[ri].is_loop = true;
                hu |= self.list(body, addr, Some(0));
                let ev = self.expr(cond, addr);
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::For { var, iter, body } => {
                self.recs[ri].kind_ok = true;
                self.recs[ri].is_loop = true;
                self.names.insert(var.clone());
                if var == &self.s {
                    self.writes.push(Wri {
                        ord,
                        full: false,
                        addr: addr.clone(),
                    });
                }
                let ev = self.expr(iter, addr);
                hu |= self.list(body, addr, Some(0));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::ForInit { init, cond, step, body } => {
                self.recs[ri].kind_ok = true;
                self.recs[ri].is_loop = true;
                hu |= self.list(init, addr, Some(0));
                let mut ev = self.expr(cond, addr);
                hu |= self.list(step, addr, Some(1));
                hu |= self.list(body, addr, Some(2));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Continue | IrStmt::Break => {
                self.recs[ri].kind_ok = true;
            }
            IrStmt::Block(b) => {
                self.recs[ri].kind_ok = true;
                hu |= self.list(b, addr, Some(0));
            }
            IrStmt::Return(e) | IrStmt::Exit(e) => {
                self.recs[ri].kind_ok = true;
                if let Some(x) = e {
                    let ev = self.expr(x, addr);
                    self.note_read(&ev, ord, addr);
                    if ev.unknown {
                        self.unknown = true;
                    }
                    self.recs[ri].pure = !ev.impure;
                }
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
                let ev = self.expr(expr, addr);
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                redirects,
                env,
            } => {
                let mut ev = self.expr(cmd, addr);
                for a in args {
                    ev.merge(self.expr(a, addr));
                }
                for r in redirects {
                    ev.merge(self.expr(r, addr));
                }
                for (n, v) in env {
                    self.names.insert(n.clone());
                    ev.merge(self.expr(v, addr));
                }
                if let Some(c) = capture {
                    self.names.insert(c.clone());
                    if c == &self.s {
                        self.writes.push(Wri {
                            ord,
                            full: false,
                            addr: addr.clone(),
                        });
                    }
                }
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Pipeline {
                stages,
                last_output,
                capture,
                cmd_str,
            } => {
                let mut ev = Ev::default();
                if let Some(c) = capture {
                    self.names.insert(c.clone());
                    if c == &self.s {
                        self.writes.push(Wri {
                            ord,
                            full: false,
                            addr: addr.clone(),
                        });
                    }
                }
                if let Some(l) = last_output {
                    self.names.insert(l.clone());
                }
                if let Some(t) = cmd_str {
                    // Reconstructed shell text: a `$s` in it is a read.
                    if mentions_str(t, &self.s) {
                        ev.mention = true;
                    }
                }
                for (k, stg) in stages.iter().enumerate() {
                    hu |= self.list(stg, addr, Some(k));
                }
                self.note_read(&ev, ord, addr);
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Output { value, target, .. } => {
                // Safe window material: no user-code path of its own
                // (a value CALL is caught by the purity flag below).
                self.recs[ri].kind_ok = true;
                let ev = self.expr(value, addr);
                if let Some(t) = target {
                    // Filehandle name — a separate namespace from vars;
                    // collect it for cursor-name collision only.
                    self.names.insert(t.clone());
                }
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::WriteFile { path, content, .. } => {
                // Same class as Output: own exprs only, purity-policing
                // any call inside them.
                self.recs[ri].kind_ok = true;
                let mut ev = self.expr(path, addr);
                ev.merge(self.expr(content, addr));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                hu |= self.list(body, addr, Some(0));
                for (k, e) in excepts.iter().enumerate() {
                    if let Some(t) = &e.as_name {
                        self.names.insert(t.clone());
                        if t == &self.s {
                            self.writes.push(Wri {
                                ord,
                                full: false,
                                addr: addr.clone(),
                            });
                        }
                    }
                    if let Some(m) = &e.match_expr {
                        let ev = self.expr(m, addr);
                        self.note_read(&ev, ord, addr);
                        if ev.unknown {
                            self.unknown = true;
                        }
                    }
                    hu |= self.list(&e.body, addr, Some(1 + k));
                }
                hu |= self.list(else_body, addr, Some(1 + excepts.len()));
                hu |= self.list(finally_body, addr, Some(2 + excepts.len()));
            }
            IrStmt::Require(_) => {}
            IrStmt::RawText(_) => {
                // Raw source text: unseeable.
                self.mark_unknown("rawtext-stmt");
            }
            IrStmt::Case { discriminant, clauses } => {
                let mut ev = self.expr(discriminant, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                // Two shapes, two treatments:
                //
                //  * a RECOGNIZED guarded front-chop is replaced
                //    outright by pass 1 (children() collapses it to its
                //    chop arm), so its discriminant read and its
                //    `*) x=$s; s=""` fallback are NOT window material —
                //    recording them would make the guard refuse itself
                //    (the read sits inside the very loop that re-runs a
                //    use, and the fallback writes `s`).
                //
                //  * any other `case` joins the region only when it is
                //    invisible to this candidate — no read, write or
                //    chop of `s` anywhere inside. A multi-channel
                //    tokenizer has one guard per channel: the sibling
                //    guards are pure, touch a different variable, and
                //    must not refuse the candidate (they used to —
                //    kind_ok stayed false for every Case, which is what
                //    kept the demo's rbx_rp/gp/bp loops unconverted).
                if guard_shape(discriminant, clauses, &self.s).is_some() {
                    self.guards.insert(addr.to_vec());
                    self.recs[ri].kind_ok = true;
                    self.recs[ri].pure = !ev.impure;
                    hu |= self.list(&clauses[0].body, addr, Some(0));
                    // clause 1 (the default arm) is never walked: it is
                    // the drain, and pass 1 drops it with the statement.
                } else {
                    let (r0, w0, u0) = (self.reads.len(), self.writes.len(), self.uses.len());
                    for (k, c) in clauses.iter().enumerate() {
                        for p in &c.patterns {
                            if mentions_str(p, &self.s) {
                                ev.mention = true;
                            }
                        }
                        hu |= self.list(&c.body, addr, Some(k));
                    }
                    self.note_read(&ev, ord, addr);
                    self.recs[ri].pure = !ev.impure;
                    if self.reads.len() == r0
                        && self.writes.len() == w0
                        && self.uses.len() == u0
                    {
                        self.recs[ri].kind_ok = true;
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                let mut ev = Ev::default();
                for r in redirects {
                    ev.merge(self.expr(&r.target, addr));
                }
                hu |= self.list(inner, addr, Some(0));
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Function {
                name,
                params,
                body,
                named_blocks,
            } => {
                self.fn_names.insert(name.clone());
                self.names.insert(name.clone());
                for p in params {
                    self.fn_params.insert(p.clone());
                    self.names.insert(p.clone());
                }
                hu |= self.list(body, addr, Some(0));
                for (k, (_, b)) in named_blocks.iter().enumerate() {
                    hu |= self.list(b, addr, Some(1 + k));
                }
            }
            IrStmt::Subshell(b) | IrStmt::Background(b) => {
                if matches!(st, IrStmt::Background(_)) {
                    self.bg_addrs.push(addr.clone());
                }
                hu |= self.list(b, addr, Some(0));
            }
            IrStmt::Select { clauses } => {
                for (k, c) in clauses.iter().enumerate() {
                    if let Some(t) = &c.target {
                        self.names.insert(t.clone());
                        if t == &self.s {
                            self.writes.push(Wri {
                                ord,
                                full: false,
                                addr: addr.clone(),
                            });
                        }
                    }
                    let mut ev = Ev::default();
                    if let Some(ch) = &c.ch {
                        ev.merge(self.expr(ch, addr));
                    }
                    if let Some(v) = &c.value {
                        ev.merge(self.expr(v, addr));
                    }
                    self.note_read(&ev, ord, addr);
                    hu |= self.list(&c.body, addr, Some(k));
                }
            }
            IrStmt::Asm { outputs, inputs, .. } => {
                // Raw machine code: unknown side effects.
                self.recs[ri].pure = false;
                let mut ev = Ev::default();
                for (_, e) in outputs {
                    ev.merge(self.expr(e, addr));
                }
                for (_, e) in inputs {
                    ev.merge(self.expr(e, addr));
                }
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
            }
            IrStmt::Expr(e) => {
                let ev = self.expr(e, addr);
                self.note_read(&ev, ord, addr);
                if ev.unknown {
                    self.unknown = true;
                }
                self.recs[ri].pure = !ev.impure;
            }
            IrStmt::Label(_) | IrStmt::Goto(_) => {}
            IrStmt::Shaderize { var, body, .. } => {
                self.names.insert(var.clone());
                if var == &self.s {
                    self.writes.push(Wri {
                        ord,
                        full: false,
                        addr: addr.clone(),
                    });
                }
                hu |= self.list(body, addr, Some(0));
            }
            IrStmt::Ext(_) => {
                // Ext stmt payloads hide expression fields children()
                // cannot expose (ForEachLine's iterable, …): unseeable.
                self.mark_unknown(&format!("ext-stmt:{}", st_ext_tag(st)));
            }
        }
        hu
    }

    // ── expressions ───────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr, addr: &mut Vec<usize>) -> Ev {
        match e {
            IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Range { .. } | IrExpr::Json(_) => Ev::default(),
            IrExpr::Str(text, _) => {
                self.note_ident_str(text);
                let mut ev = Ev::default();
                ev.mention = mentions_str(text, &self.s);
                ev
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                self.names.insert(n.clone());
                let mut ev = Ev::default();
                ev.mention = n == &self.s;
                ev
            }
            IrExpr::Index { var, key } => {
                self.names.insert(var.clone());
                self.array_vars.insert(var.clone());
                self.idx_refs.insert(var.clone());
                let mut ev = self.expr(key, addr);
                collect_idents(key, &mut self.idx_refs);
                if var == &self.s {
                    ev.mention = true;
                }
                ev
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                let mut ev = self.expr(lhs, addr);
                ev.merge(self.expr(rhs, addr));
                ev
            }
            IrExpr::Call { func, args } => self.call(func, args, addr),
            IrExpr::MethodCall { obj, args, .. } => {
                let mut ev = self.expr(obj, addr);
                for a in args {
                    ev.merge(self.expr(a, addr));
                }
                ev.impure = true;
                ev
            }
            IrExpr::Ternary { cond, then, else_ } => {
                let mut ev = self.expr(cond, addr);
                ev.merge(self.expr(then, addr));
                ev.merge(self.expr(else_, addr));
                ev
            }
            IrExpr::DefinedOr { expr, default } => {
                let mut ev = self.expr(expr, addr);
                ev.merge(self.expr(default, addr));
                ev
            }
            IrExpr::Interpolate(parts) => {
                let mut ev = Ev::default();
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Lit(t) => {
                            self.note_ident_str(t);
                            if mentions_str(t, &self.s) {
                                ev.mention = true;
                            }
                        }
                        crate::ir::InterpPart::Expr(x) => ev.merge(self.expr(x, addr)),
                    }
                }
                ev
            }
            IrExpr::Capture { expr, .. } => {
                let mut ev = self.expr(expr, addr);
                ev.impure = true;
                ev
            }
            IrExpr::Regex { pattern, flags } => {
                let mut ev = Ev::default();
                ev.mention = mentions_str(pattern, &self.s) || mentions_str(flags, &self.s);
                ev
            }
            IrExpr::RawExpr(_) => {
                // Raw code text: unseeable.
                self.mark_unknown("rawexpr");
                let mut ev = Ev::default();
                ev.unknown = true;
                ev
            }
            IrExpr::Arrow(stmts) => self.delayed(stmts, addr),
            IrExpr::Lambda { params, body } => {
                // A lambda param shadows the outer name inside the body:
                // record it so a candidate `s` with the same name is
                // refused (fn_params guard) instead of mis-attributed.
                for p in params {
                    self.names.insert(p.clone());
                    self.fn_params.insert(p.clone());
                }
                self.delayed(body, addr)
            }
            IrExpr::Array(items) => {
                let mut ev = Ev::default();
                for x in items {
                    ev.merge(self.expr(x, addr));
                }
                ev
            }
            IrExpr::ArrayComp { var, iter, elem, cond } => {
                self.names.insert(var.clone());
                if var == &self.s {
                    self.mark_unknown("arraycomp-binds-s"); // loop-bound write of s, unmodelled
                }
                let mut ev = self.expr(iter, addr);
                ev.merge(self.expr(elem, addr));
                if let Some(c) = cond {
                    ev.merge(self.expr(c, addr));
                }
                ev.impure = true;
                ev
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Object(fields) => {
                let mut ev = Ev::default();
                for (_, v) in fields {
                    ev.merge(self.expr(v, addr));
                }
                ev
            }
            IrExpr::Splice(x) => self.expr(x, addr),
            IrExpr::Ext(n) => {
                let mut ev = Ev::default();
                for c in n.children() {
                    ev.merge(self.expr(c, addr));
                }
                // Only the cursor nodes themselves (emitted by THIS
                // transform on a previous fire) are known-pure; every
                // other ext node is an unknown semantic op → impure
                // (region-refusing) but still child-visible for reads.
                // FieldStripEnd included: the fixpoint re-analyzes the
                // NEXT candidate with the previous fire's rewritten
                // statements in its region, so the drain advance must be
                // as known-pure as the plain one (it blocked rbx_rp and
                // rbx_gp behind rbx_bp's fire in MIMEcroft).
                ev.impure = !matches!(n.tag(), "FieldGrab" | "FieldStrip" | "FieldStripEnd");
                ev
            }
        }
    }

    fn call(&mut self, func: &str, args: &[IrExpr], addr: &mut Vec<usize>) -> Ev {
        let mut ev = Ev::default();
        for a in args {
            ev.merge(self.expr(a, addr));
        }
        match func {
            "param" => {
                if let Some(IrExpr::Str(op, _)) = args.first() {
                    if op == "%%" || op == "#" {
                        if let Some(IrExpr::Str(n, _)) = args.get(1) {
                            self.param_vars.insert(n.clone());
                        }
                    }
                }
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    if n == &self.s {
                        ev.mention = true; // name-arg read (no `$`)
                    }
                }
                ev.impure = false;
            }
            "getVar" | "hasVar" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    self.names.insert(n.clone());
                    if n == &self.s {
                        ev.mention = true;
                    }
                }
                ev.impure = false;
            }
            "setVar" | "assign" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    self.names.insert(n.clone());
                    if n == &self.s {
                        ev.wr = true;
                    }
                }
                ev.impure = true;
            }
            "exec" | "builtin" => {
                // `local/declare/read/unset … NAME` forms write NAME.
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if matches!(
                        cmd.as_str(),
                        "local"
                            | "declare"
                            | "typeset"
                            | "export"
                            | "readonly"
                            | "unset"
                            | "read"
                            | "readarray"
                            | "mapfile"
                    ) {
                        for a in args.iter().skip(1) {
                            let words: Vec<&IrExpr> = match a {
                                IrExpr::Array(items) => items.iter().collect(),
                                other => vec![other],
                            };
                            for w in words {
                                if let IrExpr::Str(ws, _) = w {
                                    let n = ws.split('=').next().unwrap_or("");
                                    let n = n.trim_start_matches(|c: char| {
                                        c == '$' || c == '"' || c == '\''
                                    });
                                    if !n.is_empty() {
                                        self.names.insert(n.to_string());
                                    }
                                    if n == self.s {
                                        ev.wr = true;
                                    }
                                }
                            }
                        }
                    }
                }
                ev.impure = true;
            }
            "arith" => {
                // Text-form arithmetic: BARE identifiers are reads.
                if let Some(IrExpr::Str(t, _)) = args.first() {
                    let idents = collect_ident_words(t);
                    for n in &idents {
                        self.names.insert(n.clone());
                        self.arith_refs.insert(n.clone());
                        if n == &self.s {
                            ev.mention = true;
                        }
                    }
                }
                ev.impure = false;
            }
            "test" | "atoll" | "atol" | "atoi" | "atof" | "strtol" | "strtoll" | "strtod"
            | "strlen" | "strcmp" | "strcasecmp" | "strncmp" => {
                ev.impure = false;
            }
            "setArrayAppend" => {
                // `arr+=(x)` — writes ONLY the array named by arg 1 and
                // reads exactly its (already-walked) value args: a
                // runtime primitive with no callee that could observe
                // `s` behind this walker, so the window can model it
                // exactly. That is what the `_ =>` refusal below exists
                // for — hidden inputs — not for a builtin whose whole
                // effect set is two visible arguments (the tokenize +
                // append loop is the MIMEcroft texture loader: refusing
                // it kept the N² front-chop hot loop unconverted).
                //
                // A non-literal name is NOT modelled (dynamic array
                // identity); the literal target joins the array channel
                // so an appended candidate `s` or grab target still
                // dies in blocked(), and an append TO `s` also records
                // as a partial write of `s` for the window write check.
                match args.first() {
                    Some(IrExpr::Str(n, _)) => {
                        self.names.insert(n.clone());
                        self.array_vars.insert(n.clone());
                        if n == &self.s {
                            ev.wr = true;
                        }
                        ev.impure = false;
                    }
                    _ => ev.impure = true,
                }
            }
            _ => {
                ev.impure = true;
            }
        }
        ev
    }

    fn arith(&mut self, a: &ArithAst) -> Ev {
        let mut ev = Ev::default();
        match a {
            ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
            ArithAst::Var(n) | ArithAst::Ident(n) => {
                self.names.insert(n.clone());
                self.arith_refs.insert(n.clone());
                if n == &self.s {
                    ev.mention = true;
                }
            }
            ArithAst::Index { var, key } => {
                self.names.insert(var.clone());
                self.arith_refs.insert(var.clone());
                self.idx_refs.insert(var.clone());
                if var == &self.s {
                    ev.mention = true;
                }
                ev.merge(self.arith(key));
            }
            ArithAst::Bin { lhs, rhs, .. } => {
                ev.merge(self.arith(lhs));
                ev.merge(self.arith(rhs));
            }
            ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => {
                ev.merge(self.arith(arg));
            }
            ArithAst::Cond { test, then, else_ } => {
                ev.merge(self.arith(test));
                ev.merge(self.arith(then));
                ev.merge(self.arith(else_));
            }
            ArithAst::Assign { var, rhs, .. } => {
                self.names.insert(var.clone());
                self.arith_refs.insert(var.clone());
                if var == &self.s {
                    ev.wr = true;
                }
                ev.merge(self.arith(rhs));
            }
            ArithAst::IncDec { var, .. } => {
                self.names.insert(var.clone());
                self.arith_refs.insert(var.clone());
                if var == &self.s {
                    ev.wr = true;
                }
            }
        }
        ev
    }

    fn note_ident_str(&mut self, t: &str) {
        if is_ident(t) {
            self.names.insert(t.to_string());
        }
    }

    /// Walk a delayed statement body (`$(…)` capture internals, closure
    /// bodies): fully visible for reads/writes (their stmts get real
    /// addresses under a globally-unique slot), impure (never window
    /// material), and recorded as a blocked host so no init/use/read/
    /// write of the cursor may live strictly inside one — the rewrite
    /// walk does not descend into expr-held bodies.
    fn delayed(&mut self, stmts: &[IrStmt], addr: &mut Vec<usize>) -> Ev {
        self.arrow_hosts.push(addr.clone());
        let slot = 1000 + self.arrow_seq;
        self.arrow_seq += 1;
        self.list(stmts, addr, Some(slot));
        let mut ev = Ev::default();
        ev.impure = true;
        ev
    }

    // ── validation → Edits ────────────────────────────────────────

    fn validate(&self) -> Result<Edits, String> {
        if self.unknown {
            return Err(format!(
                "unknown node in program: {}",
                self.unknown_why.as_deref().unwrap_or("?")
            ));
        }
        // Both pair halves must exist (a partial conversion would mix
        // chopped and cursor reads of `s`).
        let grabs: Vec<&Use> = self
            .uses
            .iter()
            .filter(|u| matches!(u.kind, UKind::Grab { .. }))
            .collect();
        let strips: Vec<&Use> = self
            .uses
            .iter()
            .filter(|u| matches!(u.kind, UKind::Strip { .. }))
            .collect();
        if grabs.is_empty() || strips.is_empty() {
            return Err(format!(
                "pair incomplete: {} grabs {} strips",
                grabs.len(),
                strips.len()
            ));
        }
        // Every %%/#-classified occurrence of this `s` converts or none
        // does (uses holds ALL of them by construction).
        let min_use = self
            .uses
            .iter()
            .map(|u| u.ord)
            .min()
            .ok_or_else(|| "no uses".to_string())?;
        let e1 = self
            .uses
            .iter()
            .map(|u| u.ord)
            .max()
            .ok_or_else(|| "no uses".to_string())?;

        // Init = the last full write of `s` before the first use.
        let init = self
            .writes
            .iter()
            .filter(|w| w.full && w.ord < min_use)
            .max_by_key(|w| w.ord)
            .ok_or_else(|| "no full write of s before the first use".to_string())?;

        // Loops that re-run a use (their whole body joins the region).
        let loops: Vec<&Rec> = self.recs.iter().filter(|r| r.is_loop && r.has_use).collect();

        // Window parts may not live under a background (async) job,
        // nor strictly inside a delayed body (`$(…)`/closure — the
        // rewrite walk does not descend into expr-held bodies).
        let under_bg = |a: &[usize]| self.bg_addrs.iter().any(|b| a.starts_with(b));
        let under_arrow = |a: &[usize]| {
            self.arrow_hosts
                .iter()
                .any(|h| a.len() > h.len() && a.starts_with(h))
        };
        if under_bg(&init.addr) || under_arrow(&init.addr) {
            return Err("init inside a background or delayed body".into());
        }
        for u in &self.uses {
            if under_bg(&u.addr) || under_arrow(&u.addr) {
                return Err(format!("use at ord {} inside a background/delayed body", u.ord));
            }
        }
        for r in &self.reads {
            if under_bg(&r.addr) || under_arrow(&r.addr) {
                return Err(format!(
                    "read of s at ord {} inside a background/delayed body",
                    r.ord
                ));
            }
        }
        for w in &self.writes {
            if under_bg(&w.addr) || under_arrow(&w.addr) {
                return Err(format!(
                    "write of s at ord {} inside a background/delayed body",
                    w.ord
                ));
            }
        }

        // Uses downstream of init (address order, branch-aware).
        for u in &self.uses {
            if !is_downstream(&u.addr, &init.addr) {
                return Err(format!(
                    "use at ord {} not downstream of init {}",
                    u.ord, init.ord
                ));
            }
        }

        // Init must not sit inside a loop that re-runs a use (later
        // iterations would run pre-init statements AFTER uses).
        for l in &loops {
            if l.ord < init.ord && init.ord <= l.end {
                return Err(format!(
                    "init at ord {} inside use-loop at {}..{}",
                    init.ord, l.ord, l.end
                ));
            }
        }

        // Region = (init, last-use] ∪ every use-loop's full span.
        let mut in_region = vec![false; self.next_ord.max(1)];
        for o in (init.ord + 1)..=e1 {
            if o < in_region.len() {
                in_region[o] = true;
            }
        }
        for l in &loops {
            for o in (l.ord + 1)..=l.end {
                if o < in_region.len() {
                    in_region[o] = true;
                }
            }
        }
        for r in &self.recs {
            if r.ord < in_region.len() && in_region[r.ord] {
                if !r.kind_ok || !r.pure {
                    return Err(format!(
                        "region stmt ord {} kind_ok={} pure={} [{}]",
                        r.ord, r.kind_ok, r.pure, r.label
                    ));
                }
            }
        }

        // No write of `s` inside the region (the init write itself sits
        // before it; pre-window/post-window writes are outside).
        for w in &self.writes {
            if w.ord < in_region.len() && in_region[w.ord] {
                return Err(format!("write of s inside the window (ord {})", w.ord));
            }
        }

        // Reads: init's own list only, at/before init, never inside a
        // loop that re-runs a use (stale-after-chop otherwise).
        let init_list = &init.addr[..init.addr.len() - 1];
        for r in &self.reads {
            if &r.addr[..r.addr.len() - 1] != init_list || r.ord > init.ord {
                return Err(format!(
                    "read of s at ord {} outside init's pre-window list",
                    r.ord
                ));
            }
            for l in &loops {
                if r.addr.starts_with(&l.addr) {
                    return Err(format!("read of s at ord {} inside a use-loop", r.ord));
                }
            }
        }

        // Numeric/array/shadow guards: `s` and every grab target stay
        // on the char* channel, never a shadowed name.
        let blocked = |n: &str| -> bool {
            self.arith_refs.contains(n)
                || self.idx_refs.contains(n)
                || self.array_vars.contains(n)
                || self.fn_names.contains(n)
                || self.fn_params.contains(n)
        };
        if blocked(&self.s) {
            return Err(format!(
                "s={} is numeric/array/shadow-position (arith={} idx={} arr={} fn={} param={})",
                self.s,
                self.arith_refs.contains(&self.s),
                self.idx_refs.contains(&self.s),
                self.array_vars.contains(&self.s),
                self.fn_names.contains(&self.s),
                self.fn_params.contains(&self.s)
            ));
        }
        for t in &self.grab_targets {
            if blocked(t) {
                return Err(format!(
                    "grab target {t} is numeric/array/shadow-position (arith={} idx={} arr={} fn={} param={})",
                    self.arith_refs.contains(t),
                    self.idx_refs.contains(t),
                    self.array_vars.contains(t),
                    self.fn_names.contains(t),
                    self.fn_params.contains(t)
                ));
            }
        }

        // Collision-free cursor name.
        let mut off = format!("{}_fc", self.s);
        while self.names.contains(&off) {
            off.push('_');
        }

        let mut g = BTreeMap::new();
        for u in &grabs {
            if let UKind::Grab { delim, .. } = &u.kind {
                g.insert(u.addr.clone(), delim.clone());
            }
        }
        let mut st = BTreeMap::new();
        for u in &strips {
            if let UKind::Strip { delim } = &u.kind {
                st.insert(u.addr.clone(), delim.clone());
            }
        }
        // Every self-chop under a guard advances with the DRAINING
        // variant — the dropped `s=""` arm is what makes plain
        // FieldStrip ("off unchanged on a miss") unsound here: without
        // the drain the last field would be re-grabbed forever where
        // bash reads "".
        let drains: BTreeSet<Vec<usize>> = self
            .guards
            .iter()
            .flat_map(|g| {
                self.uses
                    .iter()
                    .filter(move |u| {
                        matches!(u.kind, UKind::Strip { .. })
                            && u.addr.len() > g.len()
                            && u.addr.starts_with(g)
                    })
                    .map(|u| u.addr.clone())
            })
            .collect();
        Some(Edits {
            s: self.s.clone(),
            off,
            grabs: g,
            strips: st,
            init_addr: init.addr.clone(),
            guards: self.guards.clone(),
            drains,
        })
        .ok_or_else(|| "unreachable".to_string())
    }
}

// ── rewrite ──────────────────────────────────────────────────────────
//
// ONE walker, two passes over the same address space: pass 1 (`conv`)
// swaps grab/strip assigns in place; pass 2 inserts the cursor init
// after the init address. Conversions must finish before any insert
// (an insert shifts every later sibling's address), and pass 2 has a
// single key so the shift is harmless. Child-list slots here MUST
// match An::list's slot ids kind-for-kind — the two walks address the
// same tree.

fn grab_node(ed: &Edits, delim: &str) -> IrExpr {
    IrExpr::Ext(Box::new(FieldGrab {
        text: IrExpr::Var(ed.s.clone(), None),
        off: IrExpr::Var(ed.off.clone(), None),
        delim: delim.to_string(),
    }))
}

fn strip_node(ed: &Edits, delim: &str) -> IrExpr {
    IrExpr::Ext(Box::new(FieldStrip {
        text: IrExpr::Var(ed.s.clone(), None),
        off: IrExpr::Var(ed.off.clone(), None),
        delim: delim.to_string(),
    }))
}

/// The draining advance: same as strip_node, but a miss jumps to the
/// end of `s` (the guarded pair's `s=""` fallback).
fn strip_end_node(ed: &Edits, delim: &str) -> IrExpr {
    IrExpr::Ext(Box::new(FieldStripEnd {
        text: IrExpr::Var(ed.s.clone(), None),
        off: IrExpr::Var(ed.off.clone(), None),
        delim: delim.to_string(),
    }))
}

fn init_stmt(ed: &Edits) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: ed.off.clone(),
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::Str("0".into(), StrStyle::DoubleQuoted),
        asm: None,
    }
}

fn walk(list: &mut Vec<IrStmt>, addr: &mut Vec<usize>, ed: &Edits, conv: bool) {
    let n = list.len();
    for i in 0..n {
        addr.push(i);
        if conv {
            if let IrStmt::Assign { targets, expr, .. } = &mut list[i] {
                if let Some(d) = ed.grabs.get(addr) {
                    *expr = grab_node(ed, d);
                } else if let Some(d) = ed.strips.get(addr) {
                    *targets = vec![AssignTarget {
                        var: ed.off.clone(),
                        sigil: None,
                        indices: vec![],
                    }];
                    *expr = if ed.drains.contains(addr) {
                        strip_end_node(ed, d)
                    } else {
                        strip_node(ed, d)
                    };
                }
            }
        }
        children(&mut list[i], addr, ed, conv);
        if !conv && *addr == ed.init_addr {
            list.insert(i + 1, init_stmt(ed));
        }
        addr.pop();
    }
}

fn slot(list: &mut Vec<IrStmt>, addr: &mut Vec<usize>, ed: &Edits, conv: bool, s: usize) {
    addr.push(s);
    walk(list, addr, ed, conv);
    addr.pop();
}

fn children(st: &mut IrStmt, addr: &mut Vec<usize>, ed: &Edits, conv: bool) {
    // A recognized guarded front-chop collapses to its chop arm. The
    // default arm (`x=$s; s=""`) never survives: FieldStripEnd's drain
    // encodes it, and keeping the `case` would leave a read of the
    // FROZEN `s` in the cursor world (plus a per-chop fnmatch over the
    // whole row — the very rescan this transform exists to remove).
    // Clause 0 becomes a Block, whose child slot is 0 — the same slot id
    // An::list gave the clause, so the use addresses still match.
    if ed.guards.contains(addr) {
        if let IrStmt::Case { clauses, .. } = st {
            let body = std::mem::take(&mut clauses[0].body);
            *st = IrStmt::Block(body);
        }
    }
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            slot(then, addr, ed, conv, 0);
            for (k, (_, b)) in elsifs.iter_mut().enumerate() {
                slot(b, addr, ed, conv, 1 + k);
            }
            slot(else_, addr, ed, conv, 1 + elsifs.len());
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Shaderize { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => slot(body, addr, ed, conv, 0),
        IrStmt::ForInit { init, step, body, .. } => {
            slot(init, addr, ed, conv, 0);
            slot(step, addr, ed, conv, 1);
            slot(body, addr, ed, conv, 2);
        }
        IrStmt::Case { clauses, .. } => {
            for (k, c) in clauses.iter_mut().enumerate() {
                slot(&mut c.body, addr, ed, conv, k);
            }
        }
        IrStmt::Redirect { inner, .. } => slot(inner, addr, ed, conv, 0),
        IrStmt::Pipeline { stages, .. } => {
            for (k, stg) in stages.iter_mut().enumerate() {
                slot(stg, addr, ed, conv, k);
            }
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            slot(body, addr, ed, conv, 0);
            for (k, (_, b)) in named_blocks.iter_mut().enumerate() {
                slot(b, addr, ed, conv, 1 + k);
            }
        }
        IrStmt::Select { clauses } => {
            for (k, c) in clauses.iter_mut().enumerate() {
                slot(&mut c.body, addr, ed, conv, k);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            slot(body, addr, ed, conv, 0);
            for (k, e) in excepts.iter_mut().enumerate() {
                slot(&mut e.body, addr, ed, conv, 1 + k);
            }
            slot(else_body, addr, ed, conv, 1 + excepts.len());
            slot(finally_body, addr, ed, conv, 2 + excepts.len());
        }
        // Leaf / expr-only statements: no child lists.
        _ => {}
    }
}

/// Run both passes over the program.
fn apply(stmts: &mut Vec<IrStmt>, ed: &Edits) {
    let mut addr = Vec::new();
    walk(stmts, &mut addr, ed, true);
    let mut addr = Vec::new();
    walk(stmts, &mut addr, ed, false);
}

// ── helpers ──────────────────────────────────────────────────────────

/// Tag of an Ext STMT (diagnostic only).
fn st_ext_tag(st: &IrStmt) -> String {
    match st {
        IrStmt::Ext(n) => n.tag().to_string(),
        _ => "?".into(),
    }
}

/// `param("op", "var", "pattern")` → (op, var, pattern). The plain
/// read form `param("", v)` has two args and is never a chop.
fn param_parts(e: &IrExpr) -> Option<(&str, &str, &str)> {
    if let IrExpr::Call { func, args } = e {
        if func == "param" {
            if let (
                Some(IrExpr::Str(op, _)),
                Some(IrExpr::Str(var, _)),
                Some(IrExpr::Str(pat, _)),
            ) = (args.get(0), args.get(1), args.get(2))
            {
                return Some((op, var, pat));
            }
        }
    }
    None
}

/// Literal, non-magic, non-empty, `$`-free delimiter (the C arms do a
/// plain strstr — no glob).
fn delim_ok(d: &str) -> bool {
    !d.is_empty()
        && !d.contains('$')
        && !d
            .chars()
            .any(|c| matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '\\'))
}

/// Classify one statement as THIS candidate's front-chop half.
///
/// The single source of truth for the pair shapes: the scan records uses
/// through it and `guard_shape` recognizes a guard through it, so a guard
/// can never be recognized and then walked as a different pair (which
/// would leave a half-converted series mixing chopped and cursor reads).
fn chop_kind(st: &IrStmt, s: &str) -> Option<UKind> {
    let IrStmt::Assign { targets, expr, .. } = st else {
        return None;
    };
    if targets.len() != 1 || !targets[0].indices.is_empty() {
        return None;
    }
    let tgt = targets[0].var.as_str();
    let (op, var, pat) = param_parts(expr)?;
    if op == "%%" && var == s && tgt != s {
        // x = head of s up to first D  →  pattern "D*"
        let d = pat.strip_suffix('*')?;
        if delim_ok(d) {
            return Some(UKind::Grab {
                target: tgt.to_string(),
                delim: d.to_string(),
            });
        }
    } else if op == "#" && var == s && tgt == s {
        // s = tail after first D  →  pattern "*D"
        let d = pat.strip_prefix('*')?;
        if delim_ok(d) {
            return Some(UKind::Strip {
                delim: d.to_string(),
            });
        }
    }
    None
}

/// A plain read of `s`: `$s`, `${s}` or `getVar("s")`.
fn is_read_of(e: &IrExpr, s: &str) -> bool {
    match e {
        IrExpr::Var(v, _) => v == s,
        IrExpr::Call { func, args } if func == "getVar" => {
            matches!(args.first(), Some(IrExpr::Str(n, _)) if n == s)
        }
        _ => false,
    }
}

/// The empty value: `s=""` (a Str literal or an all-literal-empty
/// interpolation — what the frontend builds for `s=""`).
fn is_empty(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(t, _) => t.is_empty(),
        IrExpr::Interpolate(parts) => parts
            .iter()
            .all(|p| matches!(p, crate::ir::InterpPart::Lit(t) if t.is_empty())),
        _ => false,
    }
}

/// Single bare target of an assign (`targets[0].var` with no subscript).
fn assign_target<'a>(st: &'a IrStmt, ) -> Option<&'a str> {
    if let IrStmt::Assign { targets, .. } = st {
        if targets.len() == 1 && targets[0].indices.is_empty() {
            return Some(&targets[0].var);
        }
    }
    None
}

/// A GUARDED front-chop — the pair wrapped in its own delimiter test:
///
/// ```sh
/// case $s in
///   *"D"*) x=${s%%D*}; s=${s#*D} ;;   # hit:  head field + advance
///   *)     x=$s;        s=""      ;;   # miss: tail, then DRAIN
/// esac
/// ```
///
/// Returns `(grab target, delim)` when the statement is exactly that
/// shape. Every arm is pinned, refuse > guess:
///
/// * the discriminant is a plain read of `s` (anything else reads state
///   the cursor does not model);
/// * exactly two clauses, the glob arm FIRST (a `*` first would make it
///   unreachable — and pass 1's rewrite keys on clause slot 0), the
///   default arm LAST and exactly `*`;
/// * the glob arm is exactly two statements — the grab, then the strip —
///   classified by `chop_kind` with the SAME delimiter, and that
///   delimiter's `*D*` is the guard's own pattern (after dropping source
///   quotes: `*" "*` and `* *` are the same test);
/// * the default arm is exactly `x=$s; s=""` with the SAME target —
///   that `s=""` is the contract FieldStripEnd's drain has to reproduce.
fn guard_shape(disc: &IrExpr, clauses: &[IrCaseClause], s: &str) -> Option<(String, String)> {
    if !is_read_of(disc, s) || clauses.len() != 2 {
        return None;
    }
    let (chop, fallback) = (&clauses[0], &clauses[1]);
    if fallback.patterns.len() != 1 || fallback.patterns[0] != "*" {
        return None;
    }
    if chop.patterns.len() != 1 || chop.body.len() != 2 || fallback.body.len() != 2 {
        return None;
    }
    let target = match chop_kind(&chop.body[0], s)? {
        UKind::Grab { target, delim } => (target, delim),
        _ => return None,
    };
    match chop_kind(&chop.body[1], s)? {
        UKind::Strip { delim } if delim == target.1 => {}
        _ => return None,
    }
    if assign_target(&fallback.body[0])? != target.0 {
        return None;
    }
    let IrStmt::Assign { expr, .. } = &fallback.body[0] else {
        return None;
    };
    if !is_read_of(expr, s) {
        return None;
    }
    if assign_target(&fallback.body[1])? != s {
        return None;
    }
    let IrStmt::Assign { expr, .. } = &fallback.body[1] else {
        return None;
    };
    if !is_empty(expr) {
        return None;
    }
    // the guard must test the SAME delimiter its arms chop on
    let p = chop.patterns[0].replace('"', "");
    if p != format!("*{}*", target.1) {
        return None;
    }
    Some(target)
}

/// Does `text` contain `$name` / `${name…}` (word-boundary exact)?
fn mentions_str(text: &str, name: &str) -> bool {
    let nb = name.as_bytes();
    let tb = text.as_bytes();
    if nb.is_empty() {
        return false;
    }
    let mut i = 0;
    while i + 1 < tb.len() {
        if tb[i] == b'$' {
            let start = if tb.get(i + 1) == Some(&b'{') {
                i + 2
            } else {
                i + 1
            };
            if tb.len() >= start + nb.len() && &tb[start..start + nb.len()] == nb {
                let end = start + nb.len();
                if end >= tb.len() || !(tb[end].is_ascii_alphanumeric() || tb[end] == b'_') {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn is_ident(t: &str) -> bool {
    let mut cs = t.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// Short description of a statement for the refusal report.
fn st_label(st: &IrStmt) -> String {
    fn ex(e: &IrExpr) -> String {
        match e {
            IrExpr::Call { func, .. } => format!("call {func}"),
            IrExpr::Arith(_) => "arith".into(),
            IrExpr::Ext(n) => n.tag().to_string(),
            IrExpr::Var(v, _) => format!("read {v}"),
            _ => "expr".into(),
        }
    }
    match st {
        IrStmt::Assign { expr, .. } => format!("assign {}", ex(expr)),
        IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => format!("{}", ex(e)),
        IrStmt::While { cond, .. } => format!("while {}", ex(cond)),
        IrStmt::If { cond, .. } => format!("if {}", ex(cond)),
        IrStmt::Case { .. } => "case".into(),
        IrStmt::Block(_) => "block".into(),
        IrStmt::Function { name, .. } => format!("fn {name}"),
        IrStmt::Ext(n) => n.tag().to_string(),
        _ => "other".into(),
    }
}

/// Bare identifier words in an arith text (numeric-position refs).
fn collect_ident_words(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = t.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if (b[i].is_ascii_alphabetic() || b[i] == b'_')
            && (i == 0 || !b[i - 1].is_ascii_alphanumeric())
        {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(t[start..i].to_string());
        } else {
            i += 1;
        }
    }
    out
}

/// Var/Ident names anywhere in an index-key expression.
fn collect_idents(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => {
            out.insert(n.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_idents(lhs, out);
            collect_idents(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_idents(cond, out);
            collect_idents(then, out);
            collect_idents(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_idents(expr, out);
            collect_idents(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    collect_idents(x, out);
                }
            }
        }
        IrExpr::Arith(a) => collect_arith_idents(a, out),
        IrExpr::Index { key, .. } => collect_idents(key, out),
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_idents(a, out);
            }
        }
        _ => {}
    }
}

fn collect_arith_idents(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => {
            out.insert(n.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_arith_idents(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arith_idents(lhs, out);
            collect_arith_idents(rhs, out);
        }
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => collect_arith_idents(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_idents(test, out);
            collect_arith_idents(then, out);
            collect_arith_idents(else_, out);
        }
        ArithAst::Assign { rhs, .. } => collect_arith_idents(rhs, out),
        _ => {}
    }
}

/// Address-downstream: does `u` execute after `init`'s subtree?
/// Addresses are [stmt-idx, slot, stmt-idx, slot, …]; a divergence at
/// a slot position = different branches (never downstream); at a stmt
/// position = plain source order in the same list.
fn is_downstream(u: &[usize], init: &[usize]) -> bool {
    let p = u
        .iter()
        .zip(init.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if p == u.len() || p == init.len() {
        return false; // one is an ancestor of the other (leaf stmts: impossible)
    }
    if p % 2 == 1 {
        return false; // diverged at a list slot → sibling branches
    }
    u[p] > init[p]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ps(op: &str, v: &str, pat: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".into(),
            args: vec![
                IrExpr::Str(op.into(), StrStyle::DoubleQuoted),
                IrExpr::Str(v.into(), StrStyle::DoubleQuoted),
                IrExpr::Str(pat.into(), StrStyle::DoubleQuoted),
            ],
        }
    }

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.into(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn str(s: &str) -> IrExpr {
        IrExpr::Str(s.into(), StrStyle::DoubleQuoted)
    }

    /// The canonical front-chop loop: init + 3 grab/strip pairs. The
    /// back-edge reads a COUNTER (mime's shape) — a cond reading `s`
    /// itself must refuse (see refuses_when_s_read_after_chops): the
    /// cursor world never empties `s`.
    fn fire_prog() -> Vec<IrStmt> {
        let mut body = vec![
            assign("x", ps("%%", "s", "|*")),
            assign("s", ps("#", "s", "*|")),
            assign("y", ps("%%", "s", "|*")),
            assign("s", ps("#", "s", "*|")),
            assign("z", ps("%%", "s", "|*")),
            assign("s", ps("#", "s", "*|")),
        ];
        body.push(IrStmt::Output {
            value: str("done"),
            newline: true,
            target: None,
        });
        vec![
            assign("s", str("a|b|c")),
            IrStmt::While {
                cond: IrExpr::Call {
                    func: "test".into(),
                    args: vec![str("-n"), IrExpr::Var("guard".into(), None)],
                },
                body,
            },
        ]
    }

    /// Walk Assign exprs: count param(op,var) calls + ext tags.
    fn census(stmts: &[IrStmt]) -> (usize, usize, usize, Vec<String>) {
        let mut grabs = 0;
        let mut strips = 0;
        let mut params = 0;
        let mut off_targets = Vec::new();
        fn walk(
            stmts: &[IrStmt],
            grabs: &mut usize,
            strips: &mut usize,
            params: &mut usize,
            off_targets: &mut Vec<String>,
        ) {
            for st in stmts {
                match st {
                    IrStmt::Assign { targets, expr, .. } => {
                        if let IrExpr::Call { func, args } = expr {
                            if func == "param" {
                                if let IrExpr::Str(op, _) = &args[0] {
                                    if op == "%%" || op == "#" {
                                        *params += 1;
                                    }
                                }
                            }
                        }
                        if let IrExpr::Ext(n) = expr {
                            match n.tag() {
                                "FieldGrab" => *grabs += 1,
                                "FieldStrip" => {
                                    *strips += 1;
                                    for t in targets {
                                        off_targets.push(t.var.clone());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                // descend into the child lists this test program uses
                if let IrStmt::While { body, .. } = st {
                    walk(body, grabs, strips, params, off_targets);
                }
                if let IrStmt::If { then, else_, .. } = st {
                    walk(then, grabs, strips, params, off_targets);
                    walk(else_, grabs, strips, params, off_targets);
                }
            }
        }
        walk(
            stmts,
            &mut grabs,
            &mut strips,
            &mut params,
            &mut off_targets,
        );
        (grabs, strips, params, off_targets)
    }

    #[test]
    fn fires_on_front_chop_loop() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        assert!(transform(&mut p), "expected the cursor conversion to fire");
        let (grabs, strips, params, offs) = census(&p);
        assert_eq!(grabs, 3, "every head-chop converts");
        assert_eq!(strips, 3, "every self-chop converts");
        assert_eq!(params, 0, "no %%/# param call survives");
        assert!(offs.iter().all(|o| o == "s_fc"), "cursor var: {offs:?}");
        // cursor init inserted right after the full write of s
        assert!(matches!(&p[1], IrStmt::Assign { targets, expr, .. }
            if targets[0].var == "s_fc" && matches!(expr, IrExpr::Str(v, _) if v == "0")));
        // idempotent: nothing left to convert
        assert!(!transform(&mut p));
        crate::transforms::set_target(None);
    }

    fn array_append(arr: &str, val: IrExpr) -> IrStmt {
        assign(
            arr,
            IrExpr::Call {
                func: "setArrayAppend".into(),
                args: vec![str(arr), IrExpr::Array(vec![val])],
            },
        )
    }

    /// MIMEcroft's rb_tex_load: tokenize the plane, APPEND the field,
    /// counter loop. The append is a side effect on another variable
    /// whose entire effect set is its two visible args — it must not
    /// veto the window (refusing it kept the N² texture loader — the
    /// measured hottest loop — unconverted).
    #[test]
    fn fires_with_array_append_in_window() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        if let IrStmt::While { body, .. } = &mut p[1] {
            body.push(array_append("R", IrExpr::Var("x".into(), None)));
        }
        assert!(
            transform(&mut p),
            "a setArrayAppend in the window must not refuse"
        );
        let (grabs, strips, params, _) = census(&p);
        assert_eq!((grabs, strips, params), (3, 3, 0));
        crate::transforms::set_target(None);
    }

    /// …and the guard the precise append model exists to keep: a
    /// candidate `s` that is ALSO an array (`s+=(…)`) sits on the
    /// array channel, not the char* channel the cursor walks. The
    /// write is outside the window, so only the array-vars record
    /// (from the append's literal target name) can catch it.
    #[test]
    fn refuses_when_s_is_an_appended_array() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        p.push(array_append("s", str("e")));
        assert!(
            !transform(&mut p),
            "an appended candidate s must refuse (array channel)"
        );
        crate::transforms::set_target(None);
    }

    /// The cursor init must survive the FULL registry, not just this
    /// transform alone: the registry runs str-chop-cursor EARLY ("so
    /// the later analyses see the new shape"), and
    /// `redundant-store-elim` runs after it — its read-walk used to
    /// miss reads buried in `IrExpr::Ext` fields, so it saw
    /// `s_fc = "0" … s_fc = FieldStrip(…)` with nothing in between
    /// (the grabs read `s_fc` INSIDE their Ext nodes) and dropped the
    /// init. A re-entered converted function (the demo calls
    /// tex_upload_payload per texture) then started from the previous
    /// run's cursor, and the render's only backstop was the
    /// whole-string strlen clamp the pair exists to eliminate.
    #[test]
    fn cursor_init_survives_full_registry() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        crate::transforms::apply(&mut p);
        crate::transforms::set_target(None);
        let (grabs, strips, params, _) = census(&p);
        assert_eq!(
            (grabs, strips, params),
            (3, 3, 0),
            "chop shape survives the full registry"
        );
        let has_init = p.iter().any(|st| matches!(st, IrStmt::Assign { targets, expr, .. }
            if targets[0].var == "s_fc" && matches!(expr, IrExpr::Str(v, _) if v == "0")));
        assert!(
            has_init,
            "cursor init dropped by a later pass (grabs={grabs} strips={strips})"
        );
    }

    #[test]
    fn refuses_when_s_read_after_chops() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        p.push(IrStmt::Output {
            value: IrExpr::Var("s".into(), None),
            newline: true,
            target: None,
        });
        assert!(!transform(&mut p), "a post-chop read of s must refuse");
        crate::transforms::set_target(None);
    }

    #[test]
    fn refuses_write_inside_window() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        if let IrStmt::While { body, .. } = &mut p[1] {
            body.insert(2, assign("s", str("z")));
        }
        assert!(!transform(&mut p), "a mid-window write of s must refuse");
        crate::transforms::set_target(None);
    }

    #[test]
    fn refuses_init_inside_use_loop() {
        crate::transforms::set_target(Some("c"));
        // full write of s INSIDE the loop that contains the uses
        let body = vec![
            assign("s", str("a|b")),
            assign("x", ps("%%", "s", "|*")),
            assign("s", ps("#", "s", "*|")),
            assign("y", ps("%%", "s", "|*")),
            assign("s", ps("#", "s", "*|")),
        ];
        let mut p = vec![IrStmt::While {
            cond: IrExpr::Call {
                func: "test".into(),
                args: vec![str("-n"), IrExpr::Var("guard".into(), None)],
            },
            body,
        }];
        assert!(!transform(&mut p), "init inside a use-loop must refuse");
        crate::transforms::set_target(None);
    }

    #[test]
    fn refuses_numeric_target() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        // grab target x also lives in arith → numeric position
        p.push(assign(
            "k",
            IrExpr::Arith(Box::new(ArithAst::Bin {
                op: "+".into(),
                lhs: Box::new(ArithAst::Var("x".into())),
                rhs: Box::new(ArithAst::Num(1)),
            })),
        ));
        assert!(!transform(&mut p), "arith-position target must refuse");
        crate::transforms::set_target(None);
    }

    #[test]
    fn refuses_without_strip_partner() {
        crate::transforms::set_target(Some("c"));
        let body = vec![
            assign("x", ps("%%", "s", "|*")),
            assign("y", ps("%%", "s", "|*")),
        ];
        let mut p = vec![
            assign("s", str("a|b|c")),
            IrStmt::While {
                cond: IrExpr::Call {
                    func: "test".into(),
                    args: vec![str("-n"), IrExpr::Var("guard".into(), None)],
                },
                body,
            },
        ];
        assert!(!transform(&mut p), "grabs without strips must refuse");
        crate::transforms::set_target(None);
    }

    #[test]
    fn off_name_avoids_collision() {
        crate::transforms::set_target(Some("c"));
        let mut p = fire_prog();
        p.insert(0, assign("s_fc", str("occupied")));
        assert!(transform(&mut p));
        let (_, _, _, offs) = census(&p);
        assert!(offs.iter().all(|o| o == "s_fc_"), "collided: {offs:?}");
        crate::transforms::set_target(None);
    }

    #[test]
    fn gate_requires_c_target() {
        crate::transforms::set_target(Some("perl"));
        let mut p = fire_prog();
        assert!(!transform(&mut p), "offer is C-only until others accept");
        crate::transforms::set_target(None);
        let mut p = fire_prog();
        assert!(!transform(&mut p), "unset target must not fire");
    }
}
