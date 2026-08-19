//! Capability: `cmp` over two literal files — silent (`-s`), default
//! (differ message), `-l` (per-byte octal list), `-b` (bytes as chars),
//! `-n N` (limit), `-i SKIP1[:SKIP2]` (skip). Emits GNU cmp's exact stdout
//! (verified on this system): a first byte diff → `F1 F2 differ: byte N,
//! line M`; a true prefix → `cmp: EOF on <short> after byte N, line M`
//! (short ends with \n) or `… in line M` (short doesn't) or `… which is
//! empty` (empty); `-l` lines ` N o1 o2`; `-b` appends `is o1 c1 o2 c2`.
//! Non-literal paths/globs refuse.

use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond } = ctx else {
        return None;
    };
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "cmp") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    let spec = cmp_spec(&words)?;
    if *cond {
        if !spec.quiet {
            return None; // condition position: only -s is a plain boolean
        }
        return Some(NativeEmit::Cond(cmp_boolean(&spec)));
    }
    Some(NativeEmit::Stmt(cmp_stmt(&spec)))
}

struct CmpSpec {
    quiet: bool,
    list: bool,
    bytes: bool,
    limit: Option<i64>,
    skip1: i64,
    skip2: i64,
    f1: String,
    f2: String,
}

fn cmp_spec(words: &[&IrExpr]) -> Option<CmpSpec> {
    let mut s = CmpSpec {
        quiet: false,
        list: false,
        bytes: false,
        limit: None,
        skip1: 0,
        skip2: 0,
        f1: String::new(),
        f2: String::new(),
    };
    let mut files: Vec<&IrExpr> = Vec::new();
    let mut it = words.iter();
    while let Some(w) = it.next() {
        match w {
            IrExpr::Str(t, _) if t.starts_with('-') && t.len() > 1 => {
                let f: String = t[1..].chars().filter(|c| "slb".contains(*c)).collect();
                if f.len() != t.len() - 1 {
                    match t.as_str() {
                        "-n" => {
                            s.limit =
                                it.next().and_then(|v| crate::ir::grep_lit_str(v)).and_then(|v| v.parse().ok())
                        }
                        "-i" => {
                            let spec = it.next().and_then(|v| crate::ir::grep_lit_str(v))?;
                            if let Some((a, b)) = spec.split_once(':') {
                                s.skip1 = a.parse().ok()?;
                                s.skip2 = b.parse().ok()?;
                            } else {
                                s.skip1 = spec.parse().ok()?;
                                s.skip2 = s.skip1;
                            }
                        }
                        _ => return None,
                    }
                } else {
                    s.quiet |= f.contains('s');
                    s.list |= f.contains('l');
                    s.bytes |= f.contains('b');
                }
            }
            _ => files.push(w),
        }
    }
    if files.len() != 2 {
        return None;
    }
    s.f1 = crate::ir::grep_lit_str(files[0])?;
    s.f2 = crate::ir::grep_lit_str(files[1])?;
    if s.f1.contains('*') || s.f2.contains('*') {
        return None;
    }
    Some(s)
}

fn read_fn(f: &str) -> String {
    format!("(sub {{ open(my $__h, '<', {f}) or return ''; local $/; my $c = <$__h>; close $__h; $c }})->()")
}

/// `-s` files-equal boolean (with -n limit).
fn cmp_boolean(s: &CmpSpec) -> String {
    let read1 = read_fn(&crate::ir::safe_perl_q_string(&s.f1));
    let read2 = read_fn(&crate::ir::safe_perl_q_string(&s.f2));
    format!(
        "({read1} eq {read2}{})",
        if let Some(n) = s.limit {
            format!(" || (substr({read1},0,{n}) eq substr({read2},0,{n}))")
        } else {
            String::new()
        }
    )
}

/// Full statement: compare, print the GNU message/list, set the status.
fn cmp_stmt(s: &CmpSpec) -> String {
    let f1 = crate::ir::safe_perl_q_string(&s.f1);
    let f2 = crate::ir::safe_perl_q_string(&s.f2);
    let mut out = String::new();
    out.push_str("my $__c1 = ");
    out.push_str(&read_fn(&f1));
    out.push_str("; my $__c2 = ");
    out.push_str(&read_fn(&f2));
    out.push_str(";");
    if s.skip1 != 0 || s.skip2 != 0 {
        out.push_str(&format!(" $__c1 = substr($__c1, {}); $__c2 = substr($__c2, {});", s.skip1, s.skip2));
    }
    if let Some(n) = s.limit {
        out.push_str(&format!(" $__c1 = substr($__c1, 0, {n}); $__c2 = substr($__c2, 0, {n});"));
    }
    // first-diff index (-1 = common range equal), then the GNU message rules
    out.push_str(" my $__n = length($__c1) < length($__c2) ? length($__c1) : length($__c2); my $__i = -1; for (my $__k = 0; $__k < $__n; $__k++) { if (substr($__c1,$__k,1) ne substr($__c2,$__k,1)) { $__i = $__k; last; } }");
    if s.quiet {
        out.push_str(" $main_exit_code = $CHILD_ERROR = ($__i < 0 && length($__c1) == length($__c2) ? 0 : 1);");
        return out;
    }
    // the shorter file (if lengths differ) is the EOF subject
    out.push_str(&format!(
        " my $__subj = length($__c1) < length($__c2) ? 1 : 2; my $__short = $__subj == 1 ? $__c1 : $__c2; my $__sn = $__subj == 1 ? {f1} : {f2};"
    ));
    if s.list {
        // -l: every differing byte
        out.push_str(" my $__diff = 0; for (my $__k = 0; $__k < $__n; $__k++) { if (substr($__c1,$__k,1) ne substr($__c2,$__k,1)) { $__diff = 1; printf(\" %d %o %o\\n\", $__k+1, ord(substr($__c1,$__k,1)), ord(substr($__c2,$__k,1))); } } if (length($__c1) != length($__c2)) { $__diff = 1; } $main_exit_code = $CHILD_ERROR = ($__diff ? 1 : 0);");
        return out;
    }
    // line number of the first diff (or EOF subject)
    out.push_str(" my $__line = 1 + (() = substr($__c1, 0, $__i) =~ /\\n/g);");
    let mut b = String::new();
    b.push_str(" if ($__i >= 0) { ");
    if s.bytes {
        b.push_str(&format!(
            "my $__o1 = ord(substr($__c1,$__i,1)); my $__o2 = ord(substr($__c2,$__i,1)); printf(\"%s %s differ: byte %d, line %d is %o %s %o %s\\n\", {f1}, {f2}, $__i+1, $__line, $__o1, chr($__o1), $__o2, chr($__o2)); $main_exit_code = $CHILD_ERROR = 1; "
        ));
    } else {
        b.push_str(&format!(
            "printf(\"%s %s differ: byte %d, line %d\\n\", {f1}, {f2}, $__i+1, $__line); $main_exit_code = $CHILD_ERROR = 1; "
        ));
    }
    b.push_str("} elsif (length($__c1) == length($__c2)) { $main_exit_code = $CHILD_ERROR = 0; } elsif (length($__short) == 0) { printf STDERR (\"cmp: EOF on %s which is empty\\n\", $__sn); $main_exit_code = $CHILD_ERROR = 1; } else { my $__sl = 1 + (() = $__short =~ /\\n/g); if (substr($__short, -1) eq \"\\n\") { printf STDERR (\"cmp: EOF on %s after byte %d, line %d\\n\", $__sn, length($__short), $__sl); } else { printf STDERR (\"cmp: EOF on %s after byte %d, in line %d\\n\", $__sn, length($__short), $__sl); } $main_exit_code = $CHILD_ERROR = 1; }");
    out.push_str(&b);
    out
}
