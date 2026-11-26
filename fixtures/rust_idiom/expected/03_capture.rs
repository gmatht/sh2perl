use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }
static __SH_STDIN: std::sync::Mutex<Option<Box<dyn std::io::Read + Send>>> = std::sync::Mutex::new(None);
static __SH_STDIN_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

static __SH_PIPESTATUS: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

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

fn __sh_pipe(stages: &[Vec<Word>], input: Option<&[u8]>, sink: Option<&mut Vec<u8>>) -> i32 {
    // The emitted program owns the pipeline: every stage is spawned right
    // here and the stages are connected with REAL OS pipes, so nothing is
    // handed back to `bash -c`. SIGPIPE works because the kernel closes the
    // read end — buffering a stage's bytes in a Vec between stages would
    // deadlock `yes | head`.
    if stages.is_empty() { return 0; }
    let mut input = input.map(|d| d.to_vec());
    let stdin_path = __SH_STDIN_PATH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if input.is_none() && stdin_path.is_none() {
        if let Some(mut r) = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let mut d = Vec::new();
            let _ = std::io::Read::read_to_end(&mut r, &mut d);
            input = Some(d);
        }
    }
    let mut sink = sink;
    let want_out = sink.is_some()
        || __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).is_some()
        || __SH_OUTFILE_TL.with(|v| v.borrow().is_some());
    let last = stages.len() - 1;
    if sink.is_none() && !want_out {
        // the final stage inherits fd 1 raw: flush Rust's buffered stdout
        // BEFORE spawning, so a `print!` written before the pipeline (no
        // trailing newline, so the LineWriter still holds it) reaches fd 1
        // ahead of the child's bytes, exactly as bash interleaves them
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    let mut prev: Option<std::process::ChildStdout> = None;
    let mut children: Vec<std::process::Child> = Vec::new();
    let mut spawn_err = false;
    for (i, argv) in stages.iter().enumerate() {
        let Some(prog) = argv.first() else { spawn_err = true; break; };
        let mut c = std::process::Command::new(prog.as_ref());
        for a in &argv[1..] { c.arg(a.as_ref()); }
        if i == 0 {
            if input.is_some() {
                c.stdin(std::process::Stdio::piped());
            } else if let Some(p) = &stdin_path {
                // the child must see the REAL device fd (`tty < /dev/pts/5`
                // needs isatty, which a byte pipe lacks)
                if let Ok(f) = std::fs::File::open(p) { c.stdin(std::process::Stdio::from(f)); }
            }
        } else if let Some(p) = prev.take() {
            // the previous stage's write end moves into this child, so our
            // own copy is dropped and the reader sees EOF
            c.stdin(std::process::Stdio::from(p));
        }
        if i < last || sink.is_some() || want_out { c.stdout(std::process::Stdio::piped()); }
        let mut ch = match c.spawn() { Ok(x) => x, Err(_) => { spawn_err = true; break; } };
        if i == 0 {
            if let Some(data) = input.take() {
                if let Some(mut si) = ch.stdin.take() { let _ = std::io::Write::write_all(&mut si, &data); }
            }
        }
        if i < last { prev = ch.stdout.take(); }
        children.push(ch);
    }
    let mut out = Vec::new();
    if sink.is_some() || want_out {
        // read the tail BEFORE waiting: a stage that keeps writing would
        // block on a full pipe otherwise
        if let Some(mut so) = children.last_mut().and_then(|c| c.stdout.take()) {
            let _ = std::io::Read::read_to_end(&mut so, &mut out);
        }
    }
    // bash gives PIPESTATUS one entry per stage; the last stage's status is
    // the pipeline's `$?`
    let mut statuses: Vec<i32> = Vec::new();
    for ch in children.iter_mut() {
        statuses.push(ch.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1));
    }
    {
        let mut ps = __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner());
        for st in &statuses { ps.push(*st); }
    }
    if !out.is_empty() {
        let mut used = false;
        if let Some(b) = sink.as_mut() { b.extend_from_slice(&out); used = true; }
        if !used {
            __SH_OUTFILE_TL.with(|v| { if let Some(f) = v.borrow_mut().as_mut() { let _ = std::io::Write::write_all(f, &out); used = true; } });
        }
        if !used {
            if let Some(b) = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                b.extend_from_slice(&out);
            } else {
                let _ = std::io::Write::write_all(&mut std::io::stdout(), &out);
            }
        }
    }
    if spawn_err { return 1; }
    statuses.last().copied().unwrap_or(0)
}

fn main() {
    let files = {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    __SH_RC.store(__sh_pipe(&[vec!["ls".into(), "-1".into()], vec!["wc".into(), "-l".into()]], None, None), Ordering::SeqCst);
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __cap_oldin;
    let __cap = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __cap_oldout);
    let mut __s = String::from_utf8_lossy(&__cap).to_string();
    while __s.ends_with('\n') { __s.pop(); }
    __s
};
    __sh_print_words(&[vec![format!("count={}", files).into()]], true);
}