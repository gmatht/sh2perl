use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }
static __SH_STDIN: std::sync::Mutex<Option<Box<dyn std::io::Read + Send>>> = std::sync::Mutex::new(None);

thread_local! { static __SH_VARS: std::cell::RefCell<__ShStore> = const { std::cell::RefCell::new(__ShStore { vars: std::collections::BTreeMap::new(), ro_trip: false }) }; }

static __SH_ARITH_WORD_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn __sh_print_words(ws: &[Vec<Word>], nl: bool) {
    // no echo in this program used -e: there is no unescape arm and no
    // `__sh_echo_esc` dependency, and no argument to pass for one.
    // a word's `$((…))` arith expansion FAILED (an empty positional
    // makes the expanded text a syntax error) — bash suppresses the
    // WHOLE simple command, not just the word
    if __SH_ARITH_WORD_FAIL.swap(false, Ordering::SeqCst) { return; }
    let mut s = String::new();
    let mut first = true;
    for w in ws {
        for x in w {
            if !first { s.push(' '); }
            first = false;
            s.push_str(x);
        }
    }
    if nl { s.push('\n'); }
    let mut used = false;
    __SH_OUTFILE_TL.with(|v| { if let Some(f) = v.borrow_mut().as_mut() { let _ = std::io::Write::write_all(f, s.as_bytes()); used = true; } });
    if used { return; }
    if let Some(b) = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        b.extend_from_slice(s.as_bytes());
        return;
    }
    print!("{}", s);
}

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
}
fn __sh_v_rc_trip() -> bool {
    __SH_VARS.with(|s| std::mem::replace(&mut s.borrow_mut().ro_trip, false))
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
        eprintln!("bash: {}: readonly variable", name);
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
        eprintln!("bash: {}: readonly variable", name);
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

fn __sh_v_i(name: &str) -> i64 {
    match __sh_v_get(name) {
        Some(v) => match v.val.int {
            Some(n) => n,
            None => v.val.text().trim().parse::<i64>().unwrap_or(0),
        },
        None => 0,
    }
}
fn __sh_v_set_i(name: &str, v: i64) {
    __sh_v_put(name, __ShVal { int: Some(v), ..__ShVal::default() });
}
/// In-place mutation of an int home: the store materializes the value, runs
/// the body, and writes it back through `__sh_v_put` (readonly/export apply).
fn __sh_vw_i<R>(name: &str, f: impl FnOnce(&std::cell::Cell<i64>) -> R) -> R {
    let tmp = std::cell::Cell::new(__sh_v_i(name));
    let r = f(&tmp);
    __sh_v_set_i(name, tmp.get());
    r
}

fn main() {
    __sh_v_set_i("n", 0);
    let out = {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __sh_v_set_i("n", __sh_v_i("n") + 1);
    __sh_print_words(&[vec![__sh_v_i("n").to_string().into()]], true);
    __SH_RC.store(0, Ordering::SeqCst);
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __cap_oldin;
    let __cap = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __cap_oldout);
    let mut __s = String::from_utf8_lossy(&__cap).to_string();
    while __s.ends_with('\n') { __s.pop(); }
    __s
};
    __sh_print_words(&[vec![format!("{} {}", out, "0").into()]], true);
}