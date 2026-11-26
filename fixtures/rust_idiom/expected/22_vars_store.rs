use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }
static __SH_STDIN: std::sync::Mutex<Option<Box<dyn std::io::Read + Send>>> = std::sync::Mutex::new(None);
static __SH_STDIN_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

static __SH_PIPESTATUS: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());
thread_local! { static __SH_VARS: std::cell::RefCell<__ShStore> = const { std::cell::RefCell::new(__ShStore { vars: std::collections::BTreeMap::new(), ro_trip: false, at_line: 0 }) }; }

static __SH_ARITH_WORD_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn __sh_wq(ws: &[Word]) -> String {
    let mut o = String::new();
    for (i, w) in ws.iter().enumerate() {
        if i > 0 { o.push(' '); }
        o.push('\'');
        for c in w.chars() {
            if c == '\'' { o.push_str("'\\''"); } else { o.push(c); }
        }
        o.push('\'');
    }
    o
}

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

fn __sh_puts(s: &str) {
    // constant-echo path for a program that DOES use a redirect/capture
    // sink: the caller has already joined the words and applied -n/-e,
    // so this only routes to the active sink before stdout. (A
    // runtime-free program emits a bare print! instead and never calls
    // this.)
    let mut used = false;
    __SH_OUTFILE_TL.with(|v| { if let Some(f) = v.borrow_mut().as_mut() { let _ = std::io::Write::write_all(f, s.as_bytes()); used = true; } });
    if used { return; }
    if let Some(b) = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        b.extend_from_slice(s.as_bytes());
        return;
    }
    print!("{}", s);
}

fn __sh_spawn(cmd: &str, input: Option<&[u8]>) -> i32 {
    // a shell-out inside a native stage inherits the stage's stdin
    // (`echo x | grep -f <(…)` — the grep reads the echo's buffer)
    let mut input = input.map(|d| d.to_vec());
    let stdin_path = __SH_STDIN_PATH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if input.is_none() && stdin_path.is_none() {
        if let Some(mut r) = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let mut d = Vec::new();
            let _ = std::io::Read::read_to_end(&mut r, &mut d);
            input = Some(d);
        }
    }
    let want_out = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).is_some() || __SH_OUTFILE_TL.with(|v| v.borrow().is_some());
    let mut c = std::process::Command::new("bash");
    c.arg("-c").arg(cmd);
    if !want_out {
        // the child inherits fd 1 raw — flush Rust's buffered stdout
        // first so prints BEFORE the child hit the pipe first
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    if want_out { c.stdout(std::process::Stdio::piped()); }
    if input.is_none() {
        if let Some(p) = &stdin_path {
            // the child must see the REAL device fd (`tty < /dev/pts/5`
            // needs isatty, which a byte pipe lacks)
            if let Ok(f) = std::fs::File::open(p) {
                c.stdin(std::process::Stdio::from(f));
            }
        }
    }
    if input.is_some() { c.stdin(std::process::Stdio::piped()); }
    let mut ch = match c.spawn() { Ok(x) => x, Err(_) => return 1 };
    if let Some(data) = input {
        if let Some(mut si) = ch.stdin.take() { let _ = std::io::Write::write_all(&mut si, &data); }
    }
    let mut out = Vec::new();
    if let Some(mut so) = ch.stdout.take() {
        let _ = std::io::Read::read_to_end(&mut so, &mut out);
    }
    let rc = ch.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    if !out.is_empty() {
        let mut used = false;
        __SH_OUTFILE_TL.with(|v| { if let Some(f) = v.borrow_mut().as_mut() { let _ = std::io::Write::write_all(f, &out); used = true; } });
        if !used {
            if let Some(b) = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                b.extend_from_slice(&out);
            } else {
                let _ = std::io::Write::write_all(&mut std::io::stdout(), &out);
            }
        }
    }
    rc
}

fn __sh_run(cmd: &str) -> i32 {
    __sh_spawn(cmd, None)
}

fn __sh_readline() -> (String, bool) {
    let mut buf: Vec<u8> = Vec::new();
    let mut one = [0u8; 1];
    let mut any = false;
    let src: Box<dyn std::io::Read + Send> = match __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take() {
        Some(r) => r,
        None => Box::new(std::io::stdin()),
    };
    let mut r = src;
    loop {
        match r.read(&mut one) {
            Ok(0) => break,
            Ok(_) => {
                if one[0] == b'\n' { any = true; break; }
                buf.push(one[0]);
                any = true;
            }
            Err(_) => break,
        }
    }
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    (String::from_utf8_lossy(&buf).to_string(), any)
}

fn __sh_read_fields(line: &str, ifs: &str, n: usize) -> Vec<Word> {
    if n <= 1 || ifs.is_empty() { return vec![Word::Owned(line.to_string())]; }
    let ws = ifs.chars().all(|c| c == ' ' || c == '\t' || c == '\n');
    let ch: Vec<char> = line.chars().collect();
    let is_sep = |c: char| ifs.contains(c);
    let mut fields: Vec<Word> = Vec::new();
    let mut i = 0;
    while i < ch.len() && is_sep(ch[i]) { i += 1; }
    while fields.len() < n - 1 && i < ch.len() {
        let start = i;
        while i < ch.len() && !is_sep(ch[i]) { i += 1; }
        fields.push(Word::Owned(ch[start..i].iter().collect()));
        if i >= ch.len() { break; }
        i += 1;
        if ws { while i < ch.len() && is_sep(ch[i]) { i += 1; } }
    }
    let rest: String = ch[i.min(ch.len())..].iter().collect();
    fields.push(Word::Owned(rest));
    fields
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
    /// source line of the statement being executed (`__sh_v_at`), which is
    /// what bash prefixes a refused write's diagnostic with
    at_line: u32,
}
fn __sh_v_rc_trip() -> bool {
    __SH_VARS.with(|s| std::mem::replace(&mut s.borrow_mut().ro_trip, false))
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
    __sh_v_set_s("GREETING", "hello".to_string());
    __sh_v_export("GREETING");
    __sh_v_set_s("GREETING", format!("{} world", __sh_v_s("GREETING")));
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
    __sh_print_words(&[vec![format!("greeting={}", __sh_v_s("GREETING")).into()]], true);
    __SH_RC.store(__sh_run(&__sh_wq(&["bash".into(), "-c".into(), "echo \"child=$GREETING\"".into()])), Ordering::SeqCst);
    __sh_v_set_s("LIMIT", "3".to_string());
    __sh_v_mark_readonly("LIMIT");
    __sh_v_at(15);
    __sh_v_set_s("LIMIT", "9".to_string());
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
    __sh_print_words(&[vec![format!("limit={} rc={}", __sh_v_s("LIMIT"), __SH_RC.load(Ordering::SeqCst)).into()]], true);
    __sh_v_at(17);
    __sh_v_unset("LIMIT");
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
    __sh_print_words(&[vec![format!("unset-ro={} rc={}", __sh_v_s("LIMIT"), __SH_RC.load(Ordering::SeqCst)).into()]], true);
    __sh_v_at(20);
    __sh_v_unset("GREETING");
    __sh_print_words(&[vec![format!("unset=[{}]", __sh_v_s("GREETING")).into()]], true);
    __sh_v_at(23);
    __sh_v_set_s("n", "5".to_string());
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
    __sh_print_words(&[vec![grow().into()]], true);
    __sh_print_words(&[vec![format!("afterfn={}", __sh_v_s("n")).into()]], true);
    __sh_v_at(31);
    let __sh_sv0 = __sh_v_save("n");
    __sh_v_set_s("n", "11".to_string());
    __sh_print_words(&[vec![format!("sub={}", __sh_v_s("n")).into()]], true);
    __sh_v_restore_one("n", __sh_sv0);
    __sh_print_words(&[vec![format!("aftersub={}", __sh_v_s("n")).into()]], true);
    __sh_v_at(34);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut __sh_pbuf1: Vec<u8> = Vec::new();
    {
    let __sh_oldout2 = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    __sh_puts("alpha\n");
    __SH_RC.store(0, Ordering::SeqCst);
    __sh_pbuf1 = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __sh_oldout2);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
}
    {
    let __oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(std::io::Cursor::new(__sh_pbuf1.clone())));
    let mut __sh_while_ran3 = false;
    let mut __sh_while_last4 = 0;
    while {
let (__ln, __any) = __sh_readline();
let __f = __sh_read_fields(&__ln, " \t\n", 1);
__sh_v_set_s("w", __f.first().cloned().unwrap_or_default().to_string());
__SH_RC.store(if __any { 0 } else { 1 }, Ordering::SeqCst);
__any
} {
        __sh_while_ran3 = true;
        __sh_print_words(&[vec![format!("pipe={} n={}", __sh_v_s("w"), __sh_v_s("n")).into()]], true);
        __SH_RC.store(0, Ordering::SeqCst);
        __sh_while_last4 = __SH_RC.load(Ordering::SeqCst);
    }
    if !__sh_while_ran3 { __SH_RC.store(0, Ordering::SeqCst); } else { __SH_RC.store(__sh_while_last4, Ordering::SeqCst); }
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __oldin;
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
}
    __sh_print_words(&[vec![format!("after={}", __sh_v_s("n")).into()]], true);
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
}

fn grow() -> String {
    __sh_v_set_s("n", "7".to_string());
    __SH_RC.store(0, Ordering::SeqCst);
    format!("infn={}", __sh_v_s("n"))
}