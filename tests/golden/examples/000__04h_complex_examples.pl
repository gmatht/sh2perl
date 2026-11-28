#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);

my $main_exit_code = 0;
our $CHILD_ERROR = 0;
use feature 'say';

our @__sh_args;
our $__sh_args_n;
@__sh_args = @ARGV;
$__sh_args_n = scalar(@__sh_args);
my $__argc = @ARGV;
my $__nocasematch = 0;
my $d;
my $nested_result;
my $count;
my $file;
my $size;
my @files;
my $process_result;
my $here_string_result;
my $perl_result;
my $a = '';
my $b = '';
my $__ps_tmp0 = '';
my $__ps_tmp1 = '';

# __SH_RUN_EXT_BEGIN (UU-FFI.md external-command seam)
# uu-ffi .so present: in-process coreutils via uu_run (capture = temp-file
# fd-1 redirect around the in-process call — no pipe, so no concurrent-
# drain deadlock, and no C-side wrapper .so needed). Absent: guarded
# fork/exec fallback (one-time warning). The bash -c below is
# feature-guarded — check_qx strips this block before scanning.
my $__SH_UU = 0;
my $__SH_UU_FFI;
our $__sh_uu_rc = 0;
my %__SH_UU_OK = map { $_ => 1 } qw(cat wc ls sort sed awk grep date sha256sum uname basename dirname cp);
BEGIN {
    my @cand;
    push @cand, $ENV{SH2_UU_LIB} if defined $ENV{SH2_UU_LIB};
    for my $up ('', '../', '../../', '../../../', '../../../../', '../../../../../') {
        push @cand, "${up}runtime/lib/libcoreutils_ffi.so";
    }
    push @cand, 'libcoreutils_ffi.so';
    for my $lib (@cand) {
        next unless defined $lib && -e $lib;
        my $ok = eval {
            require FFI::Platypus;
            my $ffi = FFI::Platypus->new(lib => [$lib]);
            $ffi->attach([uu_run => '__sh_uu_run_ffi'] => ['string', 'int', 'string[]'] => 'int');
            $__SH_UU_FFI = $ffi;
            $__SH_UU = 1;
            1;
        };
        last if $ok;
    }
    unless ($__SH_UU) {
        warn "sh2perl: uu-ffi .so (libcoreutils_ffi) not found — external commands fall back to fork/exec (slower)\n";
    }
}
my $__SH_UU_SEQ = 0;
sub __sh_uu_run {
    my @argv = @_;
    if ($__SH_UU && ($__SH_UU_OK{ $argv[0] // '' } // 0)) {
        $__sh_uu_rc = __sh_uu_run_ffi($argv[0], scalar(@argv), \@argv);
        return;
    }
    system(@argv);
    $__sh_uu_rc = $? >> 8;
}
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
sub __sh_pipe_capture {
    my ($cmd, $opts) = @_;
    # See __sh_pipe_run: one Perl pid for writer and reader of `…$$`.
    $cmd =~ s/\$\$/$$/g;
    if (__sh_pipe_load() && !__sh_pipe_unsupported_util($cmd)) {
        my $tmp = "/tmp/sh2_pipe_${$}_" . ($__SH_UU_SEQ++) . ".cap";
        my $rc = __sh_pipe_run_str($cmd . ' > ' . $tmp, defined $opts ? $opts : '');
        if ($rc >= 0 && $rc != 127) {
            my $out = '';
            if (open(my $rf, '<', $tmp)) { $out = do { local $/; <$rf> } // ''; close $rf; }
            unlink $tmp;
            $CHILD_ERROR = $rc;
            $out =~ s/\n+\z//;
            return $out;
        }
        unlink $tmp;
    }
    my $out = do {
        open(my $fh, '-|', 'bash', '-c', $cmd, $0) or do { $CHILD_ERROR = 127; return ''; };
        my $r = do { local $/; <$fh> };
        close $fh;
        $r;
    };
    $CHILD_ERROR = $? >> 8;
    $out =~ s/\n+\z//;
    return $out // '';
}
# __SH_RUN_EXT_END

$d = do { my $__sh_cap; $__sh_cap = do { require File::Temp; File::Temp::tempdir('sh2_ps_XXXXXX', TMPDIR => 1, CLEANUP => 0) }; $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
chdir($d) or do { warn "cd: " . $d . ": $!\n"; exit "1"; };
print "=== Complex Backtick Examples ===\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$nested_result = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print("Three wells: " . (do { my $__cap = ''; open my $__fh, '>', \$__cap or die; my $__old = select($__fh); print(('well' . "\n") x 3); $? = 0; close $__fh; select($__old); $__cap =~ s/\n+\z//; $__cap }), "\n");
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "Nested backticks: $nested_result";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$count = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n"; do { opendir(my $__h, '.') or return; my $__n = grep { !/^\.\.?$/ } readdir($__h); closedir $__h; print STDOUT "$__n\n"; }; $? = 0 << 8; $CHILD_ERROR = $? >> 8;;; } $__cap =~ s/\n+\z//; $__cap };
say "File count: $count";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ((($__nocasematch) ? lc('root') eq lc('root') : 'root' eq 'root')) {
    print "Running as root\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "Not running as root\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (("Darwin") =~ /^(?:Linux)$/) {
    print "Running on Linux\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } elsif (("Darwin") =~ /^(?:Darwin)$/) {
    print "Running on macOS\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "Running on other system\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
sub get_file_size {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    my $file = $__sh_args[0];
        my $size = join(' ', split(/\s+/, do { my $__sh_cap; my $__f = $file; if (open(my $__h, '<', $__f)) { my $__d = do { local $/; <$__h> }; close($__h); $__d //= q{}; $__sh_cap = length($__d); $? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { $__sh_cap = q{}; $? = 1 << 8; $CHILD_ERROR = $? >> 8; } $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }));
        print "File " . ($file) . " has " . ($size) . " bytes\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
get_file_size("000__01_file_directory_operations.sh");
@files = (split(/\s+/, do { __sh_pipe_capture(q{'ls' '-1' *.sh examples/*.sh 2> '/dev/null'}, "pipefail") }));
print("Shell scripts found: " . (join(' ', scalar(@files))), "\n");
for my $__file ((@files)) {
    $file = $__file;
    say "  - $file";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
__sh_pipe_run(q{echo 'apple
banana
cherry' > 'file1.txt'}, "pipefail");
$main_exit_code = $CHILD_ERROR = __sh_pipe_run(q{echo 'banana
cherry
date' > 'file2.txt'}, "pipefail");
$process_result = do { my $__capf = "/tmp/sh2_cap_1_$$"; { open(my $__capso, '>&', \*STDOUT) or die "capture: $!\n"; close(STDOUT); open(STDOUT, '>', $__capf) or die "capture: $!\n";     $__ps_tmp0 = do { my $__sh_cap; $__sh_cap = do { require File::Temp; (File::Temp::tempfile('sh2_ps_XXXXXX', TMPDIR => 1, UNLINK => 0))[1] }; $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
do { my @__sl = do { local $INPUT_RECORD_SEPARATOR = undef; open(my $__fh, '<', 'file1.txt') or do { $CHILD_ERROR = 2; () }; my $__c = <$__fh> // ''; close $__fh; split(/\n/, $__c) }; pop @__sl if @__sl && $__sl[-1] eq q{}; @__sl = sort { our ($a, $b); $a cmp $b } @__sl; open(my $__ofh, '>', $__ps_tmp0) or die "sort: $__ps_tmp0: $ERRNO"; if (@__sl) { print $__ofh join("\n", @__sl), "\n"; } close $__ofh; }; $? = 0 << 8; $CHILD_ERROR = $? >> 8;    $__ps_tmp1 = do { my $__sh_cap; $__sh_cap = do { require File::Temp; (File::Temp::tempfile('sh2_ps_XXXXXX', TMPDIR => 1, UNLINK => 0))[1] }; $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
do { my @__sl = do { local $INPUT_RECORD_SEPARATOR = undef; open(my $__fh, '<', 'file2.txt') or do { $CHILD_ERROR = 2; () }; my $__c = <$__fh> // ''; close $__fh; split(/\n/, $__c) }; pop @__sl if @__sl && $__sl[-1] eq q{}; @__sl = sort { our ($a, $b); $a cmp $b } @__sl; open(my $__ofh, '>', $__ps_tmp1) or die "sort: $__ps_tmp1: $ERRNO"; if (@__sl) { print $__ofh join("\n", @__sl), "\n"; } close $__ofh; }; $? = 0 << 8; $CHILD_ERROR = $? >> 8;    __sh_uu_run('comm', '-23', $__ps_tmp0, $__ps_tmp1);$? = ($__sh_uu_rc) << 8; $CHILD_ERROR = $? >> 8;
    if ($CHILD_ERROR == 0) {
        do { unlink($__ps_tmp0, $__ps_tmp1); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
    }
;; open(STDOUT, '>&', $__capso) or die "capture: $!\n"; close($__capso); } my $__cap = ''; if (open(my $__capfh, '<', $__capf)) { $__cap = do { local $/; <$__capfh> } // ''; close $__capfh; } unlink $__capf; $__cap =~ s/\n+\z//; $__cap };
print "Process substitution result:\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say $process_result;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$here_string_result = __sh_pipe_capture(q{'tr' 'a-z' 'A-Z' <<< "hello world"}, "pipefail");
say "Here string result: $here_string_result";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$perl_result = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     { no strict 'vars'; no warnings; print "Hello from Perl\n"; }
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "Perl result: $perl_result";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { unlink('file1.txt', 'file2.txt'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
print "=== Complex Backtick Examples Complete ===\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir("/")) {
    
} else {
    warn "cd: " . "/" . ": $!\n";
    
}
do { require File::Path; File::Path::remove_tree($d); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };

exit $? >> 8;
