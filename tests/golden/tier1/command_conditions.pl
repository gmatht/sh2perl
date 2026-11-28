#!/usr/bin/env perl
use strict;
use warnings;
our $CHILD_ERROR = 0;

use feature 'say';

my @args;

sub strHasPrefix {
    return index($_[0], $_[1]) == 0;
}
sub strHasSuffix {
    my ($__h, $__n) = @_;
    return length($__n) <= length($__h) && substr($__h, length($__h) - length($__n)) eq $__n;
}

sub __sh_fn_status {
    # A shell function called in CONDITION position is tested by its exit
    # status (`if f x; then …`), not by its return string: `return 0` means
    # success. Perl would read `return "0"` as the false string "0" and
    # invert the branch, so test the status explicitly. Mirrors the bash
    # convention the rest of the program already uses: 0 => branch taken.
    #
    # Argv: ($name, @args) — the renderer keeps the function name as the first
    # word of the call. A string is not a code ref under `use strict 'refs'`,
    # so resolve it through the symbol table.
    my $name = shift();
    no strict 'refs';
    my $rv = &{$name}(@_);
    # A function that `return`s a bare integer is signalling status; one that
    # returns something else (or nothing) is signalling via $?.
    my $st = defined($rv) && $rv =~ /^\s*-?[0-9]+\s*$/
        ? $rv + 0
        : ($? >> 8);
    $st = 1 if !defined $st;
    # This sub is injected right after the `use` block, BEFORE the shared
    # renderer's own preamble (`our $CHILD_ERROR = 0;`), so a bare
    # $CHILD_ERROR here is a `use strict` COMPILE error ("Global symbol
    # $CHILD_ERROR requires explicit package name") and the whole program
    # aborts — 051_primes printed NOTHING. Re-declaring the package global
    # inside the sub is always legal, makes the helper independent of
    # preamble ordering, and (same package) still writes the caller's
    # variable.
    our $CHILD_ERROR;
    $CHILD_ERROR = $st;
    return $st == 0 ? 1 : 0;
}

our @__sh_args;
our $__sh_args_n;
@__sh_args = @ARGV;
$__sh_args_n = scalar(@__sh_args);
my $__argc = @ARGV;
my $n;
my $m;
my $i;

# __SH_RUN_EXT_BEGIN (UU-FFI.md external-command seam)
# uu-ffi .so present: in-process coreutils via uu_run (capture = temp-file
# fd-1 redirect around the in-process call — no pipe, so no concurrent-
# drain deadlock, and no C-side wrapper .so needed). Absent: guarded
# fork/exec fallback (one-time warning). The bash -c below is
# feature-guarded — check_qx strips this block before scanning.
# ── pipeline front end (coreutils-ffi-pipeline C ABI) ──────────────
# uu_pipeline_run_str runs a LINEAR pipeline (a|b|c) with redirections
# in-process, so the Perl backend never emits `bash -c` for those shapes.
my $__SH_PIPE = 0;
my $__SH_PIPE_TRIED = 0;
# Shell single-quote a runtime value so the pipeline parser sees one argv.
sub __sh_pipe_load {
    return 1 if $__SH_PIPE;
    return 0 if $__SH_PIPE_TRIED;
    $__SH_PIPE_TRIED = 1;
    my @cand;
    push @cand, $ENV{SH2_UU_PIPELINE_LIB} if defined $ENV{SH2_UU_PIPELINE_LIB};
    for my $up ('', '../', '../../', '../../../', '../../../../', '../../../../../') {
        push @cand, "${up}runtime/lib/libcoreutils_ffi_pipeline.so";
    }
    push @cand, 'libcoreutils_ffi_pipeline.so';
    for my $lib (@cand) {
        next unless defined $lib && -e $lib;
        my $ok = eval {
            require FFI::Platypus;
            my $ffi = FFI::Platypus->new(api => 2);
            $ffi->lib($lib);
            $ffi->attach([uu_pipeline_run_str => '__sh_pipe_run_str'] => ['string','string'] => 'int');
            $ffi->attach([uu_pipeline_last_code => '__sh_pipe_last_code'] => [] => 'int');
            $__SH_PIPE = 1;
            1;
        };
        last if $ok;
    }
    # Point the process backends at the multicall helper shipped alongside.
    if ($__SH_PIPE && !defined $ENV{UU_FFI_MULTICALL}) {
        for my $up ('', '../', '../../', '../../../') {
            my $m = "${up}runtime/lib/coreutils-ffi-multicall";
            if (-x $m) { $ENV{UU_FFI_MULTICALL} = $m; last; }
        }
    }
    $__SH_PIPE;
}
sub __sh_pipe_unsupported_util {
    my ($cmd) = @_;
    # The reconstructed command quotes each stage's util name. These are not
    # in the pipeline .so (non-uutils tools / shell builtins); routing them
    # in-process is a race between rc 127 (unknown util) and rc 141 (SIGPIPE
    # from the neighbour writing into the dead stage) — 141 is a real exit
    # code, so the caller skipped the shell fallback and printed nothing
    # (parse-sq-with-for, intermittently). Send them to the shell up front.
    return $cmd =~ m{(?:^|\|)\s*'(?:perl|python3?|bc|find|strings|gunzip|gzip|zstd|bzip2|xz|tar|unzip|mapfile|readarray|ruby|node|php)'};
}
sub __sh_pipe_run {
    my ($cmd, $opts) = @_;
    # `$$` in rebuilt shell text is the SCRIPT's pid in bash (one shell for
    # the whole program). Each rebuilt command here runs in a FRESH `bash -c`,
    # whose own `$$` would differ per call — so a source `2>/tmp/e.$$` writer
    # and a later `cat /tmp/e.$$` reader (single-quoted by __sh_q, never
    # expanded) could never agree (realpath-cmdsub's stderr capture). Expand
    # to the Perl pid up front so writer, reader and `unlink("…" . ($$))`
    # all name the same file.
    $cmd =~ s/\$\$/$$/g;
    if (__sh_pipe_load() && !__sh_pipe_unsupported_util($cmd)) {
        my $rc = __sh_pipe_run_str($cmd, defined $opts ? $opts : '');
        # `-2` parse / `-3` backend; `127` is the multicall's "unknown util"
        # (not a uutils command) — run those through the shell instead.
        if ($rc >= 0 && $rc != 127) { $CHILD_ERROR = $rc; return $rc; }
    }
    # Not parseable / unknown util / no backend / no .so: run it via the shell.
    # argv0 after the command STRING ($0, not `bash`): a rebuilt command may
    # contain `$0`/`${0}` from the source (`script_dir=$(cd -- "$(dirname
    # "$0")" && pwd)`), and bash passes the script's $0 down to every subshell
    # and pipeline. With argv0 defaulting to "bash", `dirname ${0}` saw "bash"
    # → "." → pwd returned the CWD instead of the script's directory
    # (qx-var-builtin-cd).
    system('bash', '-c', $cmd, $0);
    $CHILD_ERROR = $? >> 8;
    return $CHILD_ERROR;
}
# __SH_RUN_EXT_END

# __SH_SUBSHELL_BEGIN (bash subshell in condition position)
sub __sh_subshell {
    my $code = shift;
    my $fh = select(STDOUT);
    $| = 1;
    select($fh);
    my $pid = fork;
    if (!defined $pid) {
        $code->();
        return $CHILD_ERROR;
    }
    if ($pid == 0) {
        my $ok = eval { $code->(); 1 };
        my $st = $ok ? $CHILD_ERROR : 1;
        exit($st);
    }
    waitpid($pid, 0);
    my $rc = $? >> 8;
    $CHILD_ERROR = $rc;
    $? = $rc << 8;
    return $rc;
}
# __SH_SUBSHELL_END

if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0)) {
    print "if grep success: yes\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if grep success: NO\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzznotthere') >= 0; } $__gn })->() > 0)) {
    print "if grep failure: YES\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if grep failure: no\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (!(((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzznotthere') >= 0; } $__gn })->() > 0))) {
    print "if ! grep failure: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if ! grep failure: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (!(((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0))) {
    print "if ! grep success: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if ! grep success: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0)) {
    print "elsif native-then-command: b\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "elsif native-then-command: c\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzz') >= 0; } $__gn })->() > 0)) {
    print "d\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "elsif command-then-native: e\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzz') >= 0; } $__gn })->() > 0)) {
    print "g\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } elsif (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0)) {
    print "elsif command-then-command: h\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "i\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
$n = "0";
while (int($n) < int(2)) {
    $n = int(($n + 1));
    say "while test: $n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
$m = "0";
while (!(int($m) >=int(2))) {
    $m = int(($m + 1));
    say "until test: $m";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
do { my $__ok = 1; for my $__t ('cond-flag.txt') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$i = "0";
while (!((system('bash', '-c', q{'grep' '-q' 'x' 'cond-flag.txt' 2> '/dev/null'}) == 0))) {
    __sh_pipe_run(q{echo 'x' >> 'cond-flag.txt'}, "pipefail");
    $i = int(($i + 1));
    say "while command: $i";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
do { my $__g = 'hi'; my $__m = 0; for my $__l (split(/\n/, $__g)) { if ($__l =~ /hi/) { $__m = 1; last; } } $? = ($__m ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$CHILD_ERROR == 0
}) {
    print "pipeline condition: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "pipeline condition: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
do { my $__g = 'hi'; my $__m = 0; for my $__l (split(/\n/, $__g)) { if ($__l =~ /zz/) { $__m = 1; last; } } $? = ($__m ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$CHILD_ERROR == 0
}) {
    print "pipeline condition: BAD2\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "pipeline condition: ok2\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (!(do {
do { my $__g = 'hi'; my $__m = 0; for my $__l (split(/\n/, $__g)) { if ($__l =~ /zz/) { $__m = 1; last; } } $? = ($__m ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$CHILD_ERROR == 0
})) {
    print "negated pipeline condition: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "negated pipeline condition: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
do { my $__g = 'hi'; my $__m = 0; for my $__l (split(/\n/, $__g)) { if ($__l =~ /zz/) { $__m = 1; last; } } $? = ($__m ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$CHILD_ERROR == 0
}) {
    print "e\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } elsif (do {
do { my $__g = 'hi'; my $__m = 0; for my $__l (split(/\n/, $__g)) { if ($__l =~ /hi/) { $__m = 1; last; } } $? = ($__m ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
$CHILD_ERROR == 0
}) {
    print "elsif pipeline: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "f\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if ((do {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
} && do {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
})) {
    print "and condition: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "and condition: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if ((do {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
} || do {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
})) {
    print "or condition: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "or condition: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if ((((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0) && ((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzz') >= 0; } $__gn })->() > 0))) {
    print "and of commands: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "and of commands: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if ((((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzz') >= 0; } $__gn })->() > 0) || ((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0))) {
    print "or of commands: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "or of commands: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (-f '/etc/passwd') {
    print "[-f]: file\n";
    } else {
    print "[-f]: NOFILE\n";
    }
if (!(-f '/etc/definitely-not-here')) {
    print "[-f negated]: ok\n";
    } else {
    }
print "[ -eq ]: ok\n";
if (strHasPrefix("abc", "a")) {
    print "[[ glob ]]: ok\n";
    }
if (('abc' =~ '^a')) {
    print "[[ regex ]]: ok\n";
    } else {
    }
sub f {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    return 0;
}
sub g {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    return 1;
}
if (__sh_fn_status("f", ())) {
    print "function true: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (__sh_fn_status("g", ())) {
    print "function false: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "function false: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
open(my $__sh2_duperr0, '>&', \*STDERR) or die; open(my $__sh2_dupferr0, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    if (chdir("/definitely/not/here")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/definitely/not/here" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
open(STDERR, '>&', $__sh2_dupferr0) or die; close($__sh2_dupferr0); open(STDERR, '>&', $__sh2_duperr0) or die; 
($CHILD_ERROR == 0)
}) {
    print "cd failure: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "cd failure: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
    if (chdir("/tmp")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/tmp" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    ($? >> 8) == 0
}) {
    print "cd success: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
print "true condition: ok\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "false condition: ok\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ((__sh_subshell(sub {
exit "0";
}) == 0)) {
    print "subshell true: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if ((__sh_subshell(sub {
exit "1";
}) == 0)) {
    print "subshell false: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "subshell false: ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
print("ternary arith: " . (int(((5 > 3) ? 10 : 5))), "\n");
print("ternary arith zero: " . (int((1 ? 10 : 20))), "\n");
print("ternary arith false: " . (int((0 ? 10 : 20))), "\n");
if ((__sh_subshell(sub {
exit "3";
}) == 0)) {
    print "subshell status: BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    say "subshell status: $CHILD_ERROR";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'zzz') >= 0; } $__gn })->() > 0)) {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "status after failed if: $CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (((sub { open(my $__gh, '<', '/etc/passwd') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'root') >= 0; } $__gn })->() > 0)) {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "status after true if: $CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { unlink('cond-flag.txt'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };

exit $? >> 8;
