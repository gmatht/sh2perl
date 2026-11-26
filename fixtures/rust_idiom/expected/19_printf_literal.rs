use std::sync::atomic::Ordering;
type Word = std::borrow::Cow<'static, str>;

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
    print!("{}", s);
}

fn main() {
    __sh_print_words(&[vec![format!("name={}\n", "alice").into()]], false);
    __sh_print_words(&[vec![format!("{}-{}\t{}\n", "a", "b", "c").into()]], false);
    __sh_print_words(&[vec!["100%\n".to_string().into()]], false);
    __sh_print_words(&[vec!["braces {a} and {b} stay literal\n".to_string().into()]], false);
}