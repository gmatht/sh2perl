fn __sh_re_ere(text: &str, pat: &str, ci: bool) -> bool {
    // Native ERE matcher for the subset `ere_supported` accepts: no child
    // shell, no dependency (`regex` is not in the generated program's
    // manifest, and this backend must not add one). Backtracking over a
    // parsed tree; `?`/`*`/`+` are `Rep` with min/max, `|` is `Alt`.
    #[allow(dead_code)]
    enum N {
        Chr(char),
        Any,
        Cls(bool, Vec<(char, char)>),
        Bol,
        Eol,
        Rep(Box<N>, usize, usize),
        Cat(Vec<N>),
        Alt(Vec<N>),
    }

    fn cls(p: &[char], i: &mut usize) -> N {
        let mut neg = false;
        if *i < p.len() && p[*i] == '^' { neg = true; *i += 1; }
        let mut rs: Vec<(char, char)> = Vec::new();
        let mut first = true;
        while *i < p.len() {
            let c = p[*i];
            if c == ']' && !first { *i += 1; break; }
            first = false;
            let lo = if c == '\\' && *i + 1 < p.len() { *i += 1; p[*i] } else { c };
            *i += 1;
            if *i + 1 < p.len() && p[*i] == '-' && p[*i + 1] != ']' {
                *i += 1;
                let hi = p[*i];
                *i += 1;
                rs.push((lo, hi));
            } else {
                rs.push((lo, lo));
            }
        }
        N::Cls(neg, rs)
    }

    fn seq(p: &[char], i: &mut usize) -> N {
        let mut v: Vec<N> = Vec::new();
        while *i < p.len() {
            let c = p[*i];
            if c == ')' || c == '|' { break; }
            *i += 1;
            let atom = match c {
                '(' => {
                    let inner = alt(p, i);
                    if *i < p.len() && p[*i] == ')' { *i += 1; }
                    inner
                }
                '^' => N::Bol,
                '$' => N::Eol,
                '.' => N::Any,
                '[' => cls(p, i),
                '\\' => {
                    let mut ch = '\\';
                    if *i < p.len() { ch = p[*i]; *i += 1; }
                    N::Chr(ch)
                }
                _ => N::Chr(c),
            };
            let mut node = atom;
            while *i < p.len() {
                match p[*i] {
                    '*' => { *i += 1; node = N::Rep(Box::new(node), 0, usize::MAX); }
                    '+' => { *i += 1; node = N::Rep(Box::new(node), 1, usize::MAX); }
                    '?' => { *i += 1; node = N::Rep(Box::new(node), 0, 1); }
                    _ => break,
                }
            }
            v.push(node);
        }
        N::Cat(v)
    }

    fn alt(p: &[char], i: &mut usize) -> N {
        let mut v = vec![seq(p, i)];
        while *i < p.len() && p[*i] == '|' {
            *i += 1;
            v.push(seq(p, i));
        }
        if v.len() == 1 { v.pop().unwrap_or(N::Cat(Vec::new())) } else { N::Alt(v) }
    }

    fn cat(v: &[N], t: &[char], i: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
        if v.is_empty() { return k(i); }
        let (head, rest) = v.split_first().unwrap();
        m(head, t, i, &mut |j| cat(rest, t, j, &mut *k))
    }

    fn m(n: &N, t: &[char], i: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
        match n {
            N::Chr(c) => i < t.len() && t[i] == *c && k(i + 1),
            N::Any => i < t.len() && k(i + 1),
            N::Bol => i == 0 && k(i),
            N::Eol => i == t.len() && k(i),
            N::Cls(neg, rs) => {
                if i >= t.len() { return false; }
                let c = t[i];
                let mut hit = false;
                for (lo, hi) in rs {
                    if c >= *lo && c <= *hi { hit = true; }
                }
                (hit != *neg) && k(i + 1)
            }
            N::Cat(v) => cat(v, t, i, k),
            N::Alt(v) => {
                for a in v {
                    if m(a, t, i, &mut *k) { return true; }
                }
                false
            }
            N::Rep(inner, min, max) => {
                let mut pos = vec![i];
                let mut cur = i;
                while pos.len() - 1 < *max {
                    let mut next: Option<usize> = None;
                    m(inner, t, cur, &mut |j| {
                        if j > cur && next.is_none() { next = Some(j); }
                        false
                    });
                    match next {
                        Some(j) => { pos.push(j); cur = j; }
                        None => break,
                    }
                }
                let mut at = pos.len() - 1;
                loop {
                    if at < *min { return false; }
                    if k(pos[at]) { return true; }
                    if at == 0 { return false; }
                    at -= 1;
                }
            }
        }
    }

    let p: Vec<char> = if ci {
        pat.chars().flat_map(|c| c.to_lowercase()).collect()
    } else {
        pat.chars().collect()
    };
    let t: Vec<char> = if ci {
        text.chars().flat_map(|c| c.to_lowercase()).collect()
    } else {
        text.chars().collect()
    };
    let mut i = 0usize;
    let re = alt(&p, &mut i);
    // bash's `=~` is an unanchored search: try every start position.
    for s in 0..=t.len() {
        if m(&re, &t, s, &mut |_| true) { return true; }
    }
    false
}

fn main() {
    let id = "abc123".to_string();
    if __sh_re_ere(&id, "^[a-z]+[0-9]+$", false) {
        print!("matched\n");
    } else {
        print!("no\n");
    }
}