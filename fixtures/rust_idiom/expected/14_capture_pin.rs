use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }
static __SH_STDIN: std::sync::Mutex<Option<Box<dyn std::io::Read + Send>>> = std::sync::Mutex::new(None);

static __SH_ARITH_WORD_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! { static __SHV_N: std::cell::Cell<i64> = const { std::cell::Cell::new(0) }; }

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

fn main() {
    __SHV_N.with(|v| v.set(0));
    let out = {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __SHV_N.with(|v| v.set(__SHV_N.with(|v| v.get()) + 1));
    __sh_print_words(&[vec![__SHV_N.with(|v| v.get()).to_string().into()]], true);
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