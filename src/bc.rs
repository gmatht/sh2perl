//! bc.rs — a self-contained GNU-bc-subset evaluator.
//!
//! PURE STD — no external crates, no I/O: the file compiles standalone
//! (`rustc --test src/bc.rs`). The emitter wires it in under the
//! `SH2_BC_NATIVE` gate (DEFAULT ON — the corpus oracle gates correctness;
//! set `SH2_BC_NATIVE=0` to keep the real `bc` spawn, maximal fidelity):
//! constant `$(echo EXPR | bc)` captures fold at compile time via [`eval`],
//! and this module's EXACT output format (verified against GNU bc below) is
//! the reference the runtime JS shapes must match.
//!
//! Semantics targets — verified against real GNU bc (the corpus oracle):
//!   scale-0 truncation for / % ^ and sqrt; the scale rules
//!   (add/sub: max(sa,sb); mult: min(sa+sb, max(sa,sb,cur)); div: cur);
//!   unary minus binds TIGHTER than `^` (`-2^2` → 4); `^` right-assoc;
//!   negative exponent = 1/(b^|e|) at cur scale; `0^0` → 1; sqrt scale =
//!   max(cur, arg_scale); the GNU output format (leading integer zero
//!   omitted: `0.5` → `.5`; trailing scale zeros kept: `1.50*2` → `3.00`;
//!   value zero → `0`); `scale=K` statements; `;`/newline separation.
//!
//! Anything outside the subset (variables, ibase/obase, define, scale>38
//! overflow, …) returns [`Err`] — the caller falls back to spawning the
//! real `bc`.

/// A signed fixed-point number: the real value is `v / 10^scale`.
/// i128 suffices for the corpus (sqrt of ints, powers < 10^30);
/// overflow → Err → spawn fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Num {
    v: i128,
    scale: u32,
}

fn pow10(k: u32) -> Result<i128, String> {
    10i128.checked_pow(k).ok_or_else(|| format!("bc: scale {k} overflows"))
}

impl Num {
    fn int(v: i128) -> Self {
        Num { v, scale: 0 }
    }
    /// The exact formatted output (GNU bc style).
    fn fmt(&self) -> String {
        if self.v == 0 {
            return "0".to_string();
        }
        let neg = self.v < 0;
        let mut digits = self.v.unsigned_abs().to_string();
        if self.scale > 0 {
            while digits.len() <= self.scale as usize {
                digits.insert(0, '0');
            }
            let split = digits.len() - self.scale as usize;
            let (int_part, frac) = digits.split_at(split);
            // GNU bc omits a leading integer zero: `0.5` → `.5`, `-.5`
            let int_part = if int_part == "0" { "" } else { int_part };
            return format!("{}{}.{}", if neg { "-" } else { "" }, int_part, frac);
        }
        format!("{}{}", if neg { "-" } else { "" }, digits)
    }
}

// ── lexer ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(Num),
    Ident(String),
    // multi-char ops
    EqEq,   // ==
    NotEq,  // !=
    Le,     // <=
    Ge,     // >=
    // single-char ops
    Eq, Plus, Minus, Star, Slash, Percent, Caret, LParen, RParen, Lt, Gt,
    Sep, // ; or newline
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let mut cs = src.chars().peekable();
    while let Some(&c) = cs.peek() {
        match c {
            ' ' | '\t' | '\r' => {
                cs.next();
            }
            '\n' | ';' => {
                cs.next();
                toks.push(Tok::Sep);
            }
            '0'..='9' | '.' => {
                let mut digits = String::new();
                let mut saw_dot = false;
                if c == '.' {
                    saw_dot = true;
                    cs.next();
                    // a bare "." with no digits is not a number
                    let mut ds = String::new();
                    while matches!(cs.peek(), Some(d) if d.is_ascii_digit()) {
                        ds.push(cs.next().unwrap());
                    }
                    if ds.is_empty() {
                        return Err("bc: unexpected '.'".to_string());
                    }
                    digits.push('0');
                    digits.push('.');
                    digits.push_str(&ds);
                } else {
                    while let Some(d) = cs.peek() {
                        if d.is_ascii_digit() {
                            digits.push(cs.next().unwrap());
                        } else if *d == '.' && !saw_dot {
                            saw_dot = true;
                            cs.next();
                            digits.push('.');
                        } else {
                            break;
                        }
                    }
                }
                // "5." — trailing dot with no following digits → scale 0
                let (int_s, frac_s) = match digits.split_once('.') {
                    Some((i, f)) => (i, f),
                    None => (digits.as_str(), ""),
                };
                let int_v = int_s
                    .parse::<i128>()
                    .map_err(|_| format!("bc: number too large: {digits}"))?;
                let frac_v: i128 = if frac_s.is_empty() {
                    0
                } else {
                    frac_s
                        .parse()
                        .map_err(|_| format!("bc: number too large: {digits}"))?
                };
                // 1.5 == 15/10 — combine the digits, not just the int part
                let v = int_v
                    .checked_mul(pow10(frac_s.len() as u32)?)
                    .and_then(|x| x.checked_add(frac_v))
                    .ok_or("bc: number too large")?;
                toks.push(Tok::Num(Num { v, scale: frac_s.len() as u32 }));
            }
            'a'..='z' | '_' => {
                let mut id = String::new();
                while matches!(cs.peek(), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_') {
                    id.push(cs.next().unwrap());
                }
                toks.push(Tok::Ident(id));
            }
            '(' => {
                cs.next();
                toks.push(Tok::LParen);
            }
            ')' => {
                cs.next();
                toks.push(Tok::RParen);
            }
            '+' => {
                cs.next();
                toks.push(Tok::Plus);
            }
            '-' => {
                cs.next();
                toks.push(Tok::Minus);
            }
            '*' => {
                cs.next();
                toks.push(Tok::Star);
            }
            '/' => {
                cs.next();
                toks.push(Tok::Slash);
            }
            '%' => {
                cs.next();
                toks.push(Tok::Percent);
            }
            '^' => {
                cs.next();
                toks.push(Tok::Caret);
            }
            '=' => {
                cs.next();
                if cs.peek() == Some(&'=') {
                    cs.next();
                    toks.push(Tok::EqEq);
                } else {
                    toks.push(Tok::Eq);
                }
            }
            '!' => {
                cs.next();
                if cs.peek() == Some(&'=') {
                    cs.next();
                    toks.push(Tok::NotEq);
                } else {
                    return Err("bc: unexpected '!'".to_string());
                }
            }
            '<' => {
                cs.next();
                if cs.peek() == Some(&'=') {
                    cs.next();
                    toks.push(Tok::Le);
                } else {
                    toks.push(Tok::Lt);
                }
            }
            '>' => {
                cs.next();
                if cs.peek() == Some(&'=') {
                    cs.next();
                    toks.push(Tok::Ge);
                } else {
                    toks.push(Tok::Gt);
                }
            }
            other => return Err(format!("bc: unexpected character '{other}'")),
        }
    }
    Ok(toks)
}

// ── arithmetic ───────────────────────────────────────────────────────

fn isqrt(v: i128) -> i128 {
    // floor(sqrt(v)) for v >= 0 — binary search (i128 max → ~64 iters)
    let mut lo = 0i128;
    let mut hi = 1i128;
    while hi * hi <= v {
        hi *= 2;
    }
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid * mid <= v {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn add(a: Num, b: Num) -> Result<Num, String> {
    let r = a.scale.max(b.scale);
    let av = a
        .v
        .checked_mul(pow10(r - a.scale)?)
        .ok_or("bc: overflow")?;
    let bv = b
        .v
        .checked_mul(pow10(r - b.scale)?)
        .ok_or("bc: overflow")?;
    Ok(Num {
        v: av.checked_add(bv).ok_or("bc: overflow")?,
        scale: r,
    })
}

fn mul(a: Num, b: Num, cur: u32) -> Result<Num, String> {
    // bc: result scale = min(sa+sb, max(sa, sb, cur)); truncate
    let r = (a.scale + b.scale).min(a.scale.max(b.scale).max(cur));
    let v = a
        .v
        .checked_mul(b.v)
        .ok_or("bc: overflow")?
        .checked_div(pow10(a.scale + b.scale - r)?)
        .ok_or("bc: overflow")?;
    Ok(Num { v, scale: r })
}

fn div(a: Num, b: Num, cur: u32) -> Result<Num, String> {
    if b.v == 0 {
        return Err("bc: division by zero".to_string());
    }
    // (a.v/10^sa) / (b.v/10^sb) truncated to cur: a.v*10^(sb+cur)/(b.v*10^sa)
    let num = a
        .v
        .checked_mul(pow10(b.scale + cur)?)
        .ok_or("bc: overflow")?;
    let den = b.v.checked_mul(pow10(a.scale)?).ok_or("bc: overflow")?;
    Ok(Num {
        v: num / den, // Rust / truncates toward zero — bc's scale truncation
        scale: cur,
    })
}

fn rem(a: Num, b: Num, cur: u32) -> Result<Num, String> {
    if b.v == 0 {
        return Err("bc: division by zero".to_string());
    }
    // a - trunc(a/b)*b at r = max(cur, sa, sb)
    let r = cur.max(a.scale).max(b.scale);
    let q = div(a, b, r)?;
    let qb = mul(q, b, r)?;
    add(a, qb.neg())
}

impl Num {
    fn neg(self) -> Num {
        Num { v: -self.v, scale: self.scale }
    }
}

fn pow(base: Num, exp: Num, cur: u32) -> Result<Num, String> {
    // exponent must be an integer (bc: fractional exponents error)
    if exp.scale != 0 {
        return Err("bc: fractional exponent".to_string());
    }
    let e = exp.v;
    if e == 0 {
        return Ok(Num::int(1)); // 0^0 -> 1
    }
    let neg_exp = e < 0;
    let e = e.unsigned_abs();
    // exponent as i32 (an i128 exponent would loop forever)
    if e > i32::MAX as u128 {
        return Err("bc: exponent too large".to_string());
    }
    let e = e as i32;
    let mut result = Num::int(1);
    let mut b = base;
    let mut n = e;
    while n > 0 {
        if n & 1 == 1 {
            result = mul(result, b, cur)?;
        }
        n >>= 1;
        if n > 0 {
            b = mul(b, b, cur)?;
        }
    }
    if neg_exp {
        // 1/(b^|e|) at cur scale
        div(Num::int(1), result, cur)
    } else {
        Ok(result)
    }
}

fn sqrt(a: Num, cur: u32) -> Result<Num, String> {
    if a.v < 0 {
        return Err("bc: square root of negative number".to_string());
    }
    // result scale k = max(cur, arg_scale); int = isqrt(v * 10^(2k-s))
    let k = cur.max(a.scale);
    let scaled = a
        .v
        .checked_mul(pow10(2 * k - a.scale)?)
        .ok_or("bc: overflow")?;
    Ok(Num { v: isqrt(scaled), scale: k })
}

// ── parser / evaluator ───────────────────────────────────────────────

struct Eval {
    toks: Vec<Tok>,
    pos: usize,
    cur_scale: u32,
}

impl Eval {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// power := signed_primary ('^' power)?   (right-assoc; unary minus
    /// binds tighter than ^ — `-2^2` → 4 — the RHS is a signed primary)
    fn power(&mut self) -> Result<Num, String> {
        let base = self.signed_primary()?;
        if self.peek() == Some(&Tok::Caret) {
            self.next();
            let exp = self.power()?;
            pow(base, exp, self.cur_scale)
        } else {
            Ok(base)
        }
    }

    fn signed_primary(&mut self) -> Result<Num, String> {
        if self.peek() == Some(&Tok::Minus) {
            self.next();
            Ok(self.signed_primary()?.neg())
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<Num, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(n),
            Some(Tok::LParen) => {
                let v = self.expr()?;
                if self.next() != Some(Tok::RParen) {
                    return Err("bc: expected ')'".to_string());
                }
                Ok(v)
            }
            Some(Tok::Ident(name)) if name == "sqrt" => {
                if self.next() != Some(Tok::LParen) {
                    return Err("bc: expected '(' after sqrt".to_string());
                }
                let v = self.expr()?;
                if self.next() != Some(Tok::RParen) {
                    return Err("bc: expected ')'".to_string());
                }
                sqrt(v, self.cur_scale)
            }
            other => Err(format!("bc: unexpected token {other:?}")),
        }
    }

    /// mul := power (('*'|'/'|'%') power)*
    fn mul_level(&mut self) -> Result<Num, String> {
        let mut v = self.power()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.next();
                    let b = self.power()?;
                    v = mul(v, b, self.cur_scale)?;
                }
                Some(Tok::Slash) => {
                    self.next();
                    let b = self.power()?;
                    v = div(v, b, self.cur_scale)?;
                }
                Some(Tok::Percent) => {
                    self.next();
                    let b = self.power()?;
                    v = rem(v, b, self.cur_scale)?;
                }
                _ => return Ok(v),
            }
        }
    }

    /// add := mul (('+'|'-') mul)*
    fn add_level(&mut self) -> Result<Num, String> {
        let mut v = self.mul_level()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.next();
                    let b = self.mul_level()?;
                    v = add(v, b)?;
                }
                Some(Tok::Minus) => {
                    self.next();
                    let b = self.mul_level()?;
                    v = add(v, b.neg())?;
                }
                _ => return Ok(v),
            }
        }
    }

    /// cmp := add (('=='|'!='|'<'|'>'|'<='|'>=') add)*  → 1/0
    fn cmp_level(&mut self) -> Result<Num, String> {
        let mut v = self.add_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::EqEq) | Some(Tok::NotEq) | Some(Tok::Lt) | Some(Tok::Gt)
                | Some(Tok::Le) | Some(Tok::Ge) => self.next().unwrap(),
                _ => return Ok(v),
            };
            let b = self.add_level()?;
            let res = match op {
                Tok::EqEq => v.v == b.v && v.scale == b.scale,
                Tok::NotEq => v.v != b.v || v.scale != b.scale,
                Tok::Lt => num_cmp(v, b) == std::cmp::Ordering::Less,
                Tok::Gt => num_cmp(v, b) == std::cmp::Ordering::Greater,
                Tok::Le => num_cmp(v, b) != std::cmp::Ordering::Greater,
                Tok::Ge => num_cmp(v, b) != std::cmp::Ordering::Less,
                _ => unreachable!(),
            };
            v = Num::int(res as i128);
        }
    }

    fn expr(&mut self) -> Result<Num, String> {
        self.cmp_level()
    }

    /// stmt := IDENT '=' expr  (only `scale`; ibase/obase unsupported)
    ///       | expr
    /// Returns Some(value) if the statement PRINTS (bc prints nothing for
    /// a scale assignment).
    fn stmt(&mut self) -> Result<Option<Num>, String> {
        if let Some(Tok::Ident(name)) = self.peek() {
            if name == "scale" && matches!(self.toks.get(self.pos + 1), Some(Tok::Eq)) {
                self.next(); // scale
                self.next(); // =
                let v = self.expr()?;
                // scale must be a non-negative integer (bc truncates)
                let s = v.v.checked_div(pow10(v.scale)?).ok_or("bc: overflow")?;
                if s < 0 {
                    return Err("bc: negative scale".to_string());
                }
                let s = u32::try_from(s).map_err(|_| "bc: scale too large".to_string())?;
                if s > 38 {
                    return Err("bc: scale too large".to_string());
                }
                self.cur_scale = s;
                return Ok(None);
            }
            if name == "ibase" || name == "obase" {
                return Err("bc: ibase/obase unsupported (spawn fallback)".to_string());
            }
        }
        let v = self.expr()?;
        Ok(Some(v))
    }
}

fn num_cmp(a: Num, b: Num) -> std::cmp::Ordering {
    // compare the real values: align scales
    let r = a.scale.max(b.scale);
    let av = a.v * 10i128.pow(r - a.scale);
    let bv = b.v * 10i128.pow(r - b.scale);
    av.cmp(&bv)
}

/// Evaluate a bc program (the exact text `bc` would receive on stdin) and
/// return its stdout — each expression statement's formatted value + `\n`,
/// in order (assignments print nothing). Anything outside the supported
/// subset returns [`Err`] (the caller falls back to spawning real `bc`).
pub fn eval(program: &str) -> Result<String, String> {
    let toks = lex(program)?;
    let mut ev = Eval { toks, pos: 0, cur_scale: 0 };
    let mut out = String::new();
    let mut first = true;
    loop {
        // skip leading separators
        while matches!(ev.peek(), Some(Tok::Sep)) {
            ev.next();
        }
        if ev.peek().is_none() {
            break;
        }
        if let Some(v) = ev.stmt()? {
            if !first {
                out.push('\n');
            }
            out.push_str(&v.fmt());
            first = false;
        }
        // consume the statement separator
        match ev.peek() {
            Some(Tok::Sep) => {
                ev.next();
            }
            Some(_) => return Err("bc: expected ';' or newline".to_string()),
            None => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::eval;

    fn t(prog: &str, expect: &str) {
        match eval(prog) {
            Ok(out) => assert_eq!(out, expect, "bc program {prog:?}"),
            Err(e) => panic!("bc {prog:?}: unexpected error: {e}"),
        }
    }
    fn t_err(prog: &str) {
        assert!(eval(prog).is_err(), "bc {prog:?} should error");
    }

    // every expectation below was verified against real GNU bc
    #[test]
    fn corpus_forms() {
        t("sqrt(25)", "5");
        t("sqrt(24)", "4"); // scale-0 truncation
        t("sqrt(2)", "1");
        t("sqrt(541)", "23");
        t("sqrt(10^9)", "31622");
        t("sqrt(0)", "0");
        t("sqrt(1)", "1");
        t("sqrt(0.25)", ".50"); // arg scale drives the result scale
    }

    #[test]
    fn arithmetic() {
        t("7/2", "3");
        t("7%2", "1");
        t("2^10", "1024");
        t("6/2*3", "9");
        t("2^3^2", "512"); // right-assoc
        t("2^2^3", "256");
        t("5*7", "35");
        t("5/2", "2");
        t("10/4", "2");
        t("1/3", "0");
        t("2^3*2", "16");
        t("10^0", "1");
        t("0^0", "1");
        t("3-5", "-2");
        t("-5+3", "-2");
        t("5*-2", "-10");
        t("10^18", "1000000000000000000");
        t("10000000000^2", "100000000000000000000");
    }

    #[test]
    fn precedence_and_sign() {
        t("-2^2", "4"); // unary binds tighter than ^
        t("-2^3", "-8");
        t("-(2^2)", "-4");
        t("(-2)^2", "4");
        t("2^-3", "0"); // negative exponent at scale 0
    }

    #[test]
    fn scale_rules() {
        t("1.5+1", "2.5");
        t("1.5*2", "3.0");
        t("1.50*2", "3.00");
        t("1.5*1.5", "2.2"); // mult truncates to max(sa,sb,cur)
        t("0.5*0.5", ".2");
        t("1.5^2", "2.2");
        t("1.1+2.2", "3.3");
        t("scale=0; 0.5+0.5", "1.0");
    }

    #[test]
    fn scale_statements() {
        t("scale=2; 7/3", "2.33");
        t("scale=2; 2/2", "1.00");
        t("scale=1; 10/4", "2.5");
        t("scale=5; sqrt(2)", "1.41421");
        t("scale=2; 2^-3", ".12");
        t("scale=3; 1/3", ".333");
        t("scale=1; 1.5*1.5", "2.2");
        t("scale=1; 2^3", "8");
    }

    #[test]
    fn output_format() {
        t("5.0", "5.0"); // trailing scale zeros kept
        t("5.", "5"); // trailing dot, no digits -> scale 0
        t("0.5", ".5"); // leading integer zero omitted
        t("-0.5", "-.5");
        t("-0.0", "0");
        t("0.00", "0");
        t("1.234", "1.234");
        t("scale=2; 1.234", "1.234");
    }

    #[test]
    fn multi_statement() {
        t("sqrt(25);7/2", "5\n3");
        t("sqrt(25)\n7/2", "5\n3");
        t("scale=2\n7/3", "2.33");
    }

    #[test]
    fn errors() {
        t_err("100/0");
        t_err("sqrt(-4)");
        t_err("x=5"); // unknown identifier
        t_err("ibase=16");
        t_err("1.5^2.5"); // fractional exponent
        t_err("1 +");
        t_err("sqrt");
    }
}
