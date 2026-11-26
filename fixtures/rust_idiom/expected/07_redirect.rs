use std::sync::atomic::Ordering;

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }

static __SH_ARITH_WORD_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn __sh_print_words(ws: &[Vec<String>], nl: bool, esc: bool) {
    // no echo in this program used -e, so `esc` is always false — the
    // unescape arm (and its __sh_echo_esc dependency) is compiled out.
    let _ = esc;
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
    {
        let __oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
        __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = Some(std::fs::File::create("out.txt").unwrap_or_else(|_| std::fs::File::open("/dev/null").unwrap())));
        __sh_print_words(&[vec![format!("{}\n", "a b")]], false, false);
        __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __oldout);
    }
    __sh_print_words(&[vec!["done".to_string()]], true, false);
}