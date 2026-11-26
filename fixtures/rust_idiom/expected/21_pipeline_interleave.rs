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

fn __sh_q(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    o.push('\'');
    for c in s.chars() {
        if c == '\'' { o.push_str("'\\''"); } else { o.push(c); }
    }
    o.push('\'');
    o
}

fn __sh_q_printf(s: &str) -> String {
    if s.is_empty() { return "''".to_string(); }
    let mut ansi = false;
    for c in s.chars() {
        if (c as u32) < 32 || c as u32 == 127 { ansi = true; }
    }
    if ansi {
        // control chars — bash switches to $'...' ANSI-C quoting
        let mut body = String::new();
        for c in s.chars() {
            match c {
                '\n' => body.push_str("\\n"),
                '\t' => body.push_str("\\t"),
                '\r' => body.push_str("\\r"),
                '\'' => body.push_str("\\'"),
                '\\' => body.push_str("\\\\"),
                c if (c as u32) < 32 || c as u32 == 127 => body.push_str(&format!("\\x{:02x}", c as u32)),
                _ => body.push(c),
            }
        }
        return format!("$'{}'", body);
    }
    // plain word — backslash-escape the shell metachars bash %q escapes
    let mut o = String::new();
    for c in s.chars() {
        match c {
            ' ' | '\'' | '\\' | '"' | '$' | '`' => { o.push('\\'); o.push(c); }
            _ => o.push(c),
        }
    }
    o
}

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

fn __sh_echo_esc(s: &str) -> String {
    let mut o = String::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            i += 1;
            match ch[i] {
                'n' => o.push('\n'), 't' => o.push('\t'), 'r' => o.push('\r'),
                'a' => o.push('\x07'), 'b' => o.push('\x08'), 'f' => o.push('\x0c'),
                'v' => o.push('\x0b'), 'e' => o.push('\x1b'), '\\' => o.push('\\'),
                'c' => return o,
                '0'..='7' => {
                    let mut n = 0; let mut k = 0;
                    while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                        n *= 8 + (ch[i + k] as u32 - '0' as u32); k += 1;
                    }
                    o.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                    i += k - 1;
                }
                'x' => {
                    let mut n = 0; let mut k = 0;
                    while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                        n *= 16 + ch[i + 1 + k].to_digit(16).unwrap_or(0); k += 1;
                    }
                    if k > 0 { o.push(char::from_u32(n).unwrap_or('\0')); i += k; }
                    else { o.push('x'); }
                }
                c => { o.push('\\'); o.push(c); }
            }
            i += 1;
        } else { o.push(ch[i]); i += 1; }
    }
    o
}

fn __sh_printf_b(s: &str) -> String { __sh_echo_esc(s) }

fn __sh_atoi(s: &str) -> i64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<i64>().unwrap_or(0)
}

fn __sh_atou(s: &str) -> u64 {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).unwrap_or(0);
    }
    t.parse::<u64>().unwrap_or(0)
}

fn __sh_atof(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
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

fn __sh_printf(fmt: &str, args: &[Word]) -> String {
    let mut out = String::new();
    let mut ai = 0usize;
    loop {
        let start_ai = ai;
        let ch: Vec<char> = fmt.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            let c = ch[i];
            if c == '\\' && i + 1 < ch.len() {
                i += 1;
                match ch[i] {
                    'n' => out.push('\n'), 't' => out.push('\t'), 'r' => out.push('\r'),
                    'a' => out.push('\x07'), 'b' => out.push('\x08'), 'f' => out.push('\x0c'),
                    'v' => out.push('\x0b'), 'e' => out.push('\x1b'), '\\' => out.push('\\'),
                    '0'..='7' => {
                        let mut n = 0; let mut k = 0;
                        while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                            n *= 8 + (ch[i + k] as u32 - '0' as u32); k += 1;
                        }
                        out.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                        i += k - 1;
                    }
                    'x' => {
                        let mut n = 0; let mut k = 0;
                        while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                            n *= 16 + ch[i + 1 + k].to_digit(16).unwrap_or(0); k += 1;
                        }
                        if k > 0 { out.push(char::from_u32(n).unwrap_or('\0')); i += k; }
                        else { out.push('x'); }
                    }
                    _ => { out.push('\\'); out.push(ch[i]); }
                }
                i += 1;
                continue;
            }
            if c == '%' && i + 1 < ch.len() {
                let mut j = i + 1;
                let mut flags = String::new();
                while j < ch.len() && "-+ 0#".contains(ch[j]) { flags.push(ch[j]); j += 1; }
                let mut width = String::new();
                while j < ch.len() && ch[j].is_ascii_digit() { width.push(ch[j]); j += 1; }
                let mut prec: Option<String> = None;
                if j < ch.len() && ch[j] == '.' {
                    j += 1;
                    let mut p = String::new();
                    while j < ch.len() && ch[j].is_ascii_digit() { p.push(ch[j]); j += 1; }
                    prec = Some(p);
                }
                // C LENGTH MODIFIERS (`%lld`, `%zu`, `%hhd`) — skip them:
                // an unconsumed 'l' leaked into the output verbatim
                // (corpus-c 008_sized_ints printed "4000000000ld")
                while j < ch.len() && "lhLzjt".contains(ch[j]) { j += 1; }
                if j >= ch.len() { out.push('%'); break; }
                let conv = ch[j];
                i = j + 1;
                if conv == '%' { out.push('%'); continue; }
                let arg = args.get(ai).map(|w| w.to_string()).unwrap_or_default();
                ai += 1;
                let w: usize = width.parse().unwrap_or(0);
                let left = flags.contains('-');
                let zero = flags.contains('0');
                let p = prec.as_ref().map(|s| s.parse().unwrap_or(0)).unwrap_or(-1);
                let mut piece;
                match conv {
                    's' => {
                        piece = arg.clone();
                        if p >= 0 { piece = piece.chars().take(p as usize).collect(); }
                    }
                    'b' => {
                        piece = __sh_printf_b(&arg);
                        if p >= 0 { piece = piece.chars().take(p as usize).collect(); }
                    }
                    'q' => { piece = __sh_q_printf(&arg); }
                    'c' => { piece = arg.chars().next().map(|c| c.to_string()).unwrap_or_default(); }
                    'd' | 'i' => { piece = format!("{}", __sh_atoi(&arg)); }
                    'u' => { piece = format!("{}", __sh_atou(&arg)); }
                    'o' => { piece = format!("{:o}", __sh_atou(&arg)); }
                    'x' => { piece = format!("{:x}", __sh_atou(&arg)); }
                    'X' => { piece = format!("{:X}", __sh_atou(&arg)); }
                    'f' | 'F' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*}", p2, fv);
                    }
                    'e' | 'E' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*e}", p2, fv);
                        if conv == 'E' { piece = piece.to_uppercase(); }
                    }
                    'g' | 'G' => {
                        let fv = __sh_atof(&arg);
                        let p2 = if p < 0 { 6 } else { p as usize };
                        piece = format!("{:.*}", p2, fv);
                    }
                    _ => { piece = arg.clone(); }
                }
                if w > piece.chars().count() {
                    let pad = w - piece.chars().count();
                    if left { piece = format!("{}{}", piece, " ".repeat(pad)); }
                    else if zero { piece = format!("{}{}", "0".repeat(pad), piece); }
                    else { piece = format!("{}{}", " ".repeat(pad), piece); }
                }
                out.push_str(&piece);
                continue;
            }
            out.push(c);
            i += 1;
        }
        if ai >= args.len() || ai == start_ai { break; }
    }
    out
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
    __sh_print_words(&[vec!["start:".to_string().into()]], false);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    __SH_RC.store(__sh_pipe(&[vec!["cat".into(), "alpha.txt".into()], vec!["tr".into(), "a-z".into(), "A-Z".into()], vec!["wc".into(), "-l".into()]], None, None), Ordering::SeqCst);
    __sh_print_words(&[vec!["flush:".to_string().into()]], false);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut __sh_pbuf1: Vec<u8> = Vec::new();
    {
    let __sh_oldout2 = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    __sh_print_words(&[vec![__sh_printf("%s\\n", &["one".to_string().into(), "two".to_string().into(), "three".to_string().into()]).into()]], false);
    __SH_RC.store(0, Ordering::SeqCst);
    __sh_pbuf1 = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __sh_oldout2);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
}
    __SH_RC.store(__sh_pipe(&[vec!["sort".into(), "-r".into()], vec!["head".into(), "-2".into()]], Some(&__sh_pbuf1), None), Ordering::SeqCst);
    __sh_print_words(&[vec!["glob:".to_string().into()]], false);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    __SH_RC.store(__sh_pipe(&[vec!["wc".into(), "-l".into(), "alpha.txt".into(), "beta.txt".into(), "gamma.txt".into()], vec!["grep".into(), "total".into()]], None, None), Ordering::SeqCst);
    __sh_v_set_s("two", {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut __sh_pbuf4: Vec<u8> = Vec::new();
    {
    let __sh_oldout5 = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    __sh_print_words(&[vec![__sh_printf("%s\\n", &["a".to_string().into(), "b".to_string().into(), "c".to_string().into()]).into()]], false);
    __SH_RC.store(0, Ordering::SeqCst);
    __sh_pbuf4 = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __sh_oldout5);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
}
    __SH_RC.store(__sh_pipe(&[vec!["sed".into(), "-n".into(), "2p".into()]], Some(&__sh_pbuf4), None), Ordering::SeqCst);
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __cap_oldin;
    let __cap = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __cap_oldout);
    let mut __s = String::from_utf8_lossy(&__cap).to_string();
    while __s.ends_with('\n') { __s.pop(); }
    __s
});
    __sh_print_words(&[vec![format!("two={}", __sh_v_s("two")).into()]], true);
    __sh_print_words(&[vec![format!("head:{}", {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut __sh_pbuf6: Vec<u8> = Vec::new();
    {
    let __sh_oldout7 = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    __sh_print_words(&[vec![__sh_printf("%s\\n", &["x".to_string().into(), "y".to_string().into(), "z".to_string().into()]).into()]], false);
    __SH_RC.store(0, Ordering::SeqCst);
    __sh_pbuf6 = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __sh_oldout7);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
}
    __SH_RC.store(__sh_pipe(&[vec!["tail".into(), "-1".into()]], Some(&__sh_pbuf6), None), Ordering::SeqCst);
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __cap_oldin;
    let __cap = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __cap_oldout);
    let mut __s = String::from_utf8_lossy(&__cap).to_string();
    while __s.ends_with('\n') { __s.pop(); }
    __s
}).into()]], true);
    let lastcats = {
    let __cap_oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
    let __old = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    let __cap_oldin = __SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()).take();
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    __SH_RC.store(__sh_pipe(&[vec!["cat".into(), "alpha.txt".into(), "beta.txt".into()], vec!["wc".into(), "-l".into()]], None, None), Ordering::SeqCst);
    *__SH_STDIN.lock().unwrap_or_else(|e| e.into_inner()) = __cap_oldin;
    let __cap = __SH_OUT.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    *__SH_OUT.lock().unwrap_or_else(|e| e.into_inner()) = __old;
    __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __cap_oldout);
    let mut __s = String::from_utf8_lossy(&__cap).to_string();
    while __s.ends_with('\n') { __s.pop(); }
    __s
};
    __sh_print_words(&[vec![format!("lines={}", lastcats).into()]], true);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let __sh_pbuf9: Vec<u8> = Vec::new();
    let __sh_stg10: String = format!("{} > {}", __sh_wq(&["grep".into(), "-h".into(), ".".into(), "alpha.txt".into(), "beta.txt".into()]), __sh_q("joined.txt"));
    let __sh_stg11: String = __sh_wq(&["wc".into(), "-c".into()]);
    let __sh_stg12: String = format!("{} | {}", __sh_stg10, __sh_stg11);
    __SH_RC.store(__sh_spawn(&__sh_stg12, Some(&__sh_pbuf9)), Ordering::SeqCst);
    __SH_PIPESTATUS.lock().unwrap_or_else(|e| e.into_inner()).push(__SH_RC.load(Ordering::SeqCst));
    __SH_RC.store(__sh_spawn(&format!("{} < {}", __sh_wq(&["wc".into(), "-l".into()]), __sh_q("joined.txt")), None), Ordering::SeqCst);
    __sh_print_words(&[vec!["end\n".to_string().into()]], false);
    __SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 }, Ordering::SeqCst);
}