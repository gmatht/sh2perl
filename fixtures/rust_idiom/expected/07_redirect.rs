use std::sync::atomic::Ordering;

static __SH_OUT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
thread_local! { static __SH_OUTFILE_TL: std::cell::RefCell<Option<std::fs::File>> = const { std::cell::RefCell::new(None) }; }

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
                        n *= 16 + ch[i + 1 + k].to_digit(16).unwrap(); k += 1;
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
    if let Some(b) = __SH_OUT.lock().unwrap().as_mut() {
        b.extend_from_slice(s.as_bytes());
        return;
    }
    print!("{}", s);
}

fn __sh_printf(fmt: &str, args: &[String]) -> String {
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
                    'c' => return out,
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
                            n *= 16 + ch[i + 1 + k].to_digit(16).unwrap(); k += 1;
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
                let arg = args.get(ai).cloned().unwrap_or_default();
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

fn main() {
    {
        let __oldout = __SH_OUTFILE_TL.with(|v| v.borrow_mut().take());
        __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = Some(std::fs::File::create("out.txt").unwrap_or_else(|_| std::fs::File::open("/dev/null").unwrap())));
        __sh_print_words(&[vec![__sh_printf("%s\\n", &vec!["a b".to_string()])]], false, false);
        __SH_OUTFILE_TL.with(|v| *v.borrow_mut() = __oldout);
    }
    __sh_print_words(&[vec!["done".to_string()]], true, false);
}