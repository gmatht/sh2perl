use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

thread_local! { static __SH_VARS: std::cell::RefCell<__ShStore> = const { std::cell::RefCell::new(__ShStore { vars: std::collections::BTreeMap::new(), ro_trip: false, unwind: false, at_line: 0 }) }; }

/// One stored value: the shell-visible text plus whichever typed payload
/// the renderer chose for the name. `text` reads every payload, so the value
/// type is complete in every program without a dead field.
#[derive(Clone, Default)]
struct __ShVal {
    s: String,
    int: Option<i64>,
    words: Option<Vec<Word>>,
    ints: Option<Vec<i64>>,
    assoc: Option<std::collections::BTreeMap<String, String>>,
}
impl __ShVal {
    fn text(&self) -> String {
        if let Some(n) = self.int { return n.to_string(); }
        if let Some(v) = &self.ints { return v.first().map(|n| n.to_string()).unwrap_or_default(); }
        if let Some(w) = &self.words { return w.first().map(|x| x.to_string()).unwrap_or_default(); }
        if let Some(m) = &self.assoc { return m.get("0").cloned().unwrap_or_default(); }
        self.s.clone()
    }
}

/// One store entry: the value plus the two attributes a shell keeps per
/// name (exported, readonly). The store is the ONE home of every variable
/// the promotion slice could not turn into a `let`.
#[derive(Clone)]
struct __ShVar {
    val: __ShVal,
    exported: bool,
    readonly: bool,
}
impl __ShVar {
    fn empty() -> __ShVar {
        __ShVar { val: __ShVal::default(), exported: false, readonly: false }
    }
}

/// The store itself: every unpromoted name plus the one piece of `$?`-adjacent
/// state a refused write leaves behind. `ro_trip` is consumed by the next
/// status write, so `$?` is 1 exactly until the next command runs — what bash
/// reports for a failed assignment (`x=1` on a readonly name).
struct __ShStore {
    vars: std::collections::BTreeMap<String, __ShVar>,
    ro_trip: bool,
    /// set by a REFUSED ASSIGNMENT: bash answers one by unwinding every
    /// enclosing compound command back to the top level (the shell keeps
    /// running, the statement's status is 1). Cleared at the top level, so a
    /// refused `unset` — which bash fails without unwinding — never sets it.
    unwind: bool,
    /// source line of the statement being executed (`__sh_v_at`), which is
    /// what bash prefixes a refused write's diagnostic with
    at_line: u32,
}
fn __sh_v_rc_trip() -> bool {
    __SH_VARS.with(|s| std::mem::replace(&mut s.borrow_mut().ro_trip, false))
}

/// A refused assignment unwinds the enclosing compound commands: the body it
/// happened in, then every body around it, up to the top-level statement list.
/// Each emitted body asks this at the end of every statement and leaves itself
/// when it is set; the top level clears it, so the shell carries on with the
/// refused assignment's status — exactly what bash does.
fn __sh_v_unwind_pending() -> bool {
    __SH_VARS.with(|s| s.borrow().unwind)
}
fn __sh_v_unwind_clear() {
    __SH_VARS.with(|s| s.borrow_mut().unwind = false);
}
/// Re-arm the status trip as the function returns to its caller: bash's
/// function ended on the refused assignment, so the CALL's status is 1 and
/// the caller's own status write reads it from here.
fn __sh_v_unwind_trip() {
    __SH_VARS.with(|s| s.borrow_mut().ro_trip = true);
}

/// The diagnostic prefix a shell diagnostic carries: bash prints `$0`, and
/// the emitted program's `$0` is its own argv[0].
fn __sh_prog() -> String {
    std::env::args().next().unwrap_or_default()
}
/// Record the source line of the statement whose store write can be refused
/// (`readonly`). Bash's message is `<prog>: line N: <name>: …`; the renderer
/// knows N and hands it here, so no accessor formats a location itself.
fn __sh_v_at(line: u32) {
    __SH_VARS.with(|s| s.borrow_mut().at_line = line);
}
fn __sh_v_where() -> String {
    let line = __SH_VARS.with(|s| s.borrow().at_line);
    if line == 0 {
        // no source line in the IR for this statement: print the bare prefix
        // rather than a wrong one (a nested statement in the current markup)
        __sh_prog()
    } else {
        format!("{}: line {}", __sh_prog(), line)
    }
}

fn __sh_v_get(name: &str) -> Option<__ShVar> {
    __SH_VARS.with(|s| s.borrow().vars.get(name).cloned())
}

/// Write one value through the store. A readonly name keeps its value and
/// fails the way bash fails (`<name>: readonly variable`, status 1); an
/// exported name re-syncs the process environment so children inherit the
/// new value.
fn __sh_v_put(name: &str, val: __ShVal) {
    let mut denied = false;
    __SH_VARS.with(|s| {
        let mut s = s.borrow_mut();
        let st = &mut *s;
        let e = st.vars.entry(name.to_string()).or_insert_with(__ShVar::empty);
        if e.readonly {
            st.ro_trip = true;
            st.unwind = true;
            denied = true;
            return;
        }
        e.val = val;
        if e.exported {
            let t = e.val.text();
            std::env::set_var(name, t);
        }
    });
    if denied {
        eprintln!("{}: {}: readonly variable", __sh_v_where(), name);
        __SH_RC.store(1, Ordering::SeqCst);
    }
}

/// Save one name's whole entry (value plus flags) for a scope that must put
/// it back afterwards — a subshell, a background capture, a dynamic scope.
fn __sh_v_save(name: &str) -> Option<__ShVar> {
    __sh_v_get(name)
}
fn __sh_v_restore_one(name: &str, saved: Option<__ShVar>) {
    __SH_VARS.with(|s| {
        let mut s = s.borrow_mut();
        match saved {
            Some(v) => { s.vars.insert(name.to_string(), v); }
            None => { s.vars.remove(name); }
        }
    });
}

/// `unset NAME` — a readonly name keeps its entry and fails like bash.
fn __sh_v_unset(name: &str) {
    let mut denied = false;
    __SH_VARS.with(|s| {
        let mut s = s.borrow_mut();
        let st = &mut *s;
        match st.vars.get(name) {
            Some(e) if e.readonly => {
                st.ro_trip = true;
                denied = true;
            }
            Some(_) => { st.vars.remove(name); }
            None => {}
        }
    });
    if denied {
        eprintln!(
            "{}: unset: {}: cannot unset: readonly variable",
            __sh_v_where(),
            name
        );
        __SH_RC.store(1, Ordering::SeqCst);
    }
}

/// `export NAME` — the flag lives in the store, so every later write keeps
/// the child environment in sync.
fn __sh_v_export(name: &str) {
    let mut val = None;
    __SH_VARS.with(|s| {
        let mut s = s.borrow_mut();
        let e = s.vars.entry(name.to_string()).or_insert_with(__ShVar::empty);
        e.exported = true;
        val = Some(e.val.text());
    });
    if let Some(v) = val { std::env::set_var(name, v); }
}

/// `readonly NAME` / `declare -r` — refusal happens on every later write.
fn __sh_v_mark_readonly(name: &str) {
    __SH_VARS.with(|s| {
        let mut s = s.borrow_mut();
        s.vars.entry(name.to_string()).or_insert_with(__ShVar::empty).readonly = true;
    });
}

fn __sh_v_w(name: &str) -> Vec<Word> {
    match __sh_v_get(name) {
        Some(v) => {
            if let Some(w) = v.val.words { return w; }
            if let Some(n) = v.val.ints {
                return n.iter().map(|i| Word::Owned(i.to_string())).collect();
            }
            if let Some(m) = v.val.assoc {
                return m.values().map(|x| Word::Owned(x.clone())).collect();
            }
            vec![Word::Owned(v.val.text())]
        }
        None => Vec::new(),
    }
}
fn __sh_v_set_w(name: &str, v: Vec<Word>) {
    __sh_v_put(name, __ShVal { words: Some(v), ..__ShVal::default() });
}
/// One indexed element (`${a[i]}`): the caller has already applied bash's
/// bounds rule, so an out-of-range read is the empty word, never a panic.
fn __sh_v_w_at(name: &str, i: usize) -> Word {
    __sh_v_w(name).into_iter().nth(i).unwrap_or_else(|| Word::Owned(String::new()))
}
fn __sh_vw_w<R>(name: &str, f: impl FnOnce(&std::cell::RefCell<Vec<Word>>) -> R) -> R {
    let tmp = std::cell::RefCell::new(__sh_v_w(name));
    let r = f(&tmp);
    __sh_v_set_w(name, tmp.into_inner());
    r
}

/// `${#name}`: text length for a scalar, element count for an array or map.
fn __sh_v_len(name: &str) -> i64 {
    match __sh_v_get(name) {
        Some(v) => {
            if let Some(w) = &v.val.words { return w.len() as i64; }
            if let Some(n) = &v.val.ints { return n.len() as i64; }
            if let Some(m) = &v.val.assoc { return m.len() as i64; }
            v.val.text().len() as i64
        }
        None => 0,
    }
}

fn main() {
    __sh_v_set_w("arr", vec!["1".into(), "2".into(), "3".into()]);
    let mut sum: i64 = 0;
    for x in ["1", "2", "3"] {
        sum += x.trim().parse::<i64>().unwrap_or(0);
    }
    println!("sum={} len={}", sum, (__sh_v_len("arr")));
}