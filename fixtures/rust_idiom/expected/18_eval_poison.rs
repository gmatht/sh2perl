use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

thread_local! { static __SH_VARS: std::cell::RefCell<__ShStore> = const { std::cell::RefCell::new(__ShStore { vars: std::collections::BTreeMap::new(), ro_trip: false }) }; }

static __SH_ARITH_ERR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

fn __sh_v_s(name: &str) -> String {
    match __sh_v_get(name) { Some(v) => v.val.text(), None => String::new() }
}
fn __sh_v_set_s<T: ToString>(name: &str, v: T) {
    __sh_v_put(name, __ShVal { s: v.to_string(), ..__ShVal::default() });
}
fn __sh_vw_s<R>(name: &str, f: impl FnOnce(&std::cell::RefCell<String>) -> R) -> R {
    let tmp = std::cell::RefCell::new(__sh_v_s(name));
    let r = f(&tmp);
    __sh_v_set_s(name, tmp.into_inner());
    r
}

fn main() {
    __sh_v_set_s("n", "0".to_string());
    __sh_v_set_s("n", { let __v = (__sh_v_i("n") + 1i64).to_string(); let __e = __SH_ARITH_ERR.swap(false, Ordering::SeqCst); __SH_RC.store(if __e { 1 } else { 0 }, Ordering::SeqCst); if __e { String::new() } else { __v } });
    __sh_v_set_s("n", format!("{}", (__sh_v_i("n") + 10)));
    println!("{}", __sh_v_s("n"));
}