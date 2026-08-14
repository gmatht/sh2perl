use crate::ast::*;
use crate::generator::Generator;

/// Native Perl `cmp` (byte-wise file comparison) — GNU format compatibility
/// for the corpus gate (070_cmp_basic).  check_qx.pl forbids `system()` for
/// builtins like cmp, so we emulate it:
///
///   identical                  → exit 0, no output
///   first differing byte N, M  → "F1 F2 differ: byte N, line M"  (exit 1)
///   EOF on shorter file        → "cmp: EOF on F after byte N, in line M"
///                                (or "which is empty" for an empty file)
///   -s silent                  → exit code only
///   -l verbose                 → " N o1 o2" per differing byte (octal)
///   -b print-bytes             → "differ: byte N, line M is D C D C"
///   -n LIMIT                   → compare at most LIMIT bytes (EOF within the
///                                limit reports as a DIFFER at shorter_len+1)
///   -i SKIP | -i S1:S2         → skip bytes before comparing
pub fn generate_cmp_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut silent = false;
    let mut verbose = false; // -l
    let mut print_bytes = false; // -b
    let mut limit: Option<String> = None;
    let mut skip1: String = "0".to_string();
    let mut skip2: String = "0".to_string();
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < cmd.args.len() {
        let arg = &cmd.args[i];
        if let Word::Literal(s, _) = arg {
            if s == "-s" {
                silent = true;
            } else if s == "-l" {
                verbose = true;
            } else if s == "-b" {
                print_bytes = true;
            } else if s == "-n" {
                if let Some(Word::Literal(v, _)) = cmd.args.get(i + 1) {
                    limit = Some(v.clone());
                    i += 1;
                }
            } else if let Some(stripped) = s.strip_prefix("-n") {
                if !stripped.is_empty() {
                    limit = Some(stripped.to_string());
                }
            } else if s == "-i" {
                if let Some(Word::Literal(v, _)) = cmd.args.get(i + 1) {
                    if let Some((a, b)) = v.split_once(':') {
                        skip1 = a.to_string();
                        skip2 = b.to_string();
                    } else {
                        skip1 = v.clone();
                        skip2 = v.clone();
                    }
                    i += 1;
                }
            } else if let Some(stripped) = s.strip_prefix("-i") {
                if !stripped.is_empty() {
                    if let Some((a, b)) = stripped.split_once(':') {
                        skip1 = a.to_string();
                        skip2 = b.to_string();
                    } else {
                        skip1 = stripped.to_string();
                        skip2 = stripped.to_string();
                    }
                }
            } else if s.starts_with('-') {
                // Other flags: ignore.
            } else {
                files.push(generator.word_to_perl(arg));
            }
        } else {
            files.push(generator.word_to_perl(arg));
        }
        i += 1;
    }

    if files.len() < 2 {
        // cmp with fewer than 2 files: GNU prints "cmp: missing operand" and
        // exits 2.
        return format!(
            "do {{ print STDERR q{{cmp: missing operand\n}}; $CHILD_ERROR = 2; $CHILD_ERROR; }}"
        );
    }

    let f1 = &files[0];
    let f2 = &files[1];

    let limit_expr = match &limit {
        Some(l) => format!("{}", l),
        None => "undef".to_string(),
    };

    let (l_mode, b_mode, s_mode) = if verbose {
        ("1", "0", if silent { "1" } else { "0" })
    } else {
        ("0", if print_bytes { "1" } else { "0" }, if silent { "1" } else { "0" })
    };

    format!(
        r#"do {{
    my $__f1 = {};
    my $__f2 = {};
    my $__limit = {};
    my $__skip1 = {};
    my $__skip2 = {};
    my $__mode = '{}';   # l=verbose, b=print-bytes, n=plain
    my $__silent = {};
    my $__b1 = q{{}};
    if (open my $__fh, '<', $__f1) {{ local $INPUT_RECORD_SEPARATOR = undef; $__b1 = <$__fh>; close $__fh; }}
    my $__b2 = q{{}};
    if (open my $__fh, '<', $__f2) {{ local $INPUT_RECORD_SEPARATOR = undef; $__b2 = <$__fh>; close $__fh; }}
    substr($__b1, 0, $__skip1, q{{}});
    substr($__b2, 0, $__skip2, q{{}});
    if (defined $__limit) {{
        $__b1 = substr($__b1, 0, $__limit);
        $__b2 = substr($__b2, 0, $__limit);
    }}
    my $__len = length($__b1) < length($__b2) ? length($__b1) : length($__b2);
    my $__diff = -1;
    for my $__j (0..$__len-1) {{
        if (substr($__b1, $__j, 1) ne substr($__b2, $__j, 1)) {{ $__diff = $__j; last; }}
    }}
    if ($__diff >= 0) {{
        my $__pos = $__diff + 1;
        my $__before = substr($__b1, 0, $__diff);
        my $__line = ($__before =~ tr/\n//) + 1;
        $CHILD_ERROR = 1;
        if (!$__silent) {{
            if ($__mode eq 'l') {{
                for my $__j (0..$__len-1) {{
                    if (substr($__b1, $__j, 1) ne substr($__b2, $__j, 1)) {{
                        printf " %d %o %o\n", $__j+1, ord(substr($__b1,$__j,1)), ord(substr($__b2,$__j,1));
                    }}
                }}
            }} elsif ($__mode eq 'b') {{
                my $c1 = substr($__b1, $__diff, 1);
                my $c2 = substr($__b2, $__diff, 1);
                printf "%s %s differ: byte %d, line %d is %o %s %o %s\n", $__f1, $__f2, $__pos, $__line, ord($c1), (ord($c1) >= 32 && ord($c1) <= 126 ? $c1 : q{{}}), ord($c2), (ord($c2) >= 32 && ord($c2) <= 126 ? $c2 : q{{}});
            }} else {{
                print "$__f1 $__f2 differ: byte $__pos, line $__line\n";
            }}
        }}
    }} elsif (length($__b1) != length($__b2)) {{
        # EOF on the shorter file.  With -n LIMIT a shorter file reports as a
        # DIFFER at shorter_len+1; without a limit it reports "EOF on ...".
        my $__shorter = length($__b1) < length($__b2) ? $__f1 : $__f2;
        my $__n = length($__b1) < length($__b2) ? length($__b1) : length($__b2);
        $CHILD_ERROR = 1;
        if (!$__silent) {{
            if (defined $__limit) {{
                my $__pos = $__n + 1;
                my $__before = substr((length($__b1) < length($__b2) ? $__b1 : $__b2), 0, $__n);
                my $__line = ($__before =~ tr/\n//) + 1;
                if ($__mode eq 'l') {{ }} else {{ print "$__f1 $__f2 differ: byte $__pos, line $__line\n"; }}
            }} elsif ($__n == 0) {{
                print STDERR "cmp: EOF on $__shorter which is empty\n";
            }} elsif ($__mode eq 'l') {{
                print STDERR "cmp: EOF on $__shorter after byte $__n\n";
            }} else {{
                my $__before = substr((length($__b1) < length($__b2) ? $__b1 : $__b2), 0, $__n);
                my $__line = ($__before =~ tr/\n//) + 1;
                print STDERR "cmp: EOF on $__shorter after byte $__n, in line $__line\n";
            }}
        }}
    }} else {{
        $CHILD_ERROR = 0;
    }}
    $CHILD_ERROR;
}};"#,
        f1, f2, limit_expr, skip1, skip2, if verbose { "l" } else if print_bytes { "b" } else { "n" },
        s_mode
    )
}
