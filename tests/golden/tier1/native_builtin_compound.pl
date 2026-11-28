#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);

our $CHILD_ERROR = 0;
my $x;
my $y;

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
        # bash prints nothing here: the fallback must be SILENT (a notice on
        # stderr is itself a divergence). Opt in with SH2_UU_WARN=1.
        warn "sh2perl: uu-ffi .so (libcoreutils_ffi) not found — external commands fall back to fork/exec (slower)\n"
            if $ENV{SH2_UU_WARN};
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

open(my $__ofh0, '>', 'hay.txt') or die "echo: 'hay.txt': $ERRNO"; print $__ofh0 q{alpha
}; close $__ofh0; $? = 0 << 8; $CHILD_ERROR = $? >> 8;if (chdir("/no/such/dir")) { print "cd-and-ran\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { warn "cd: " . "/no/such/dir" . ": $!\n"; $? = 1 << 8; $CHILD_ERROR = $? >> 8; }
say "after-cd-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir(".")) { print "cd-and-ran\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { warn "cd: " . "." . ": $!\n"; $? = 1 << 8; $CHILD_ERROR = $? >> 8; }
say "after-cd-ok=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "true-and-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "false-and-never\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "false-or-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "true-or-never\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
if (do {
    if (chdir(".")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "." . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    ($? >> 8) == 0
}) {
    print "if-cd-hit\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if-cd-miss\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (do {
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    ($? >> 8) == 0
}) {
    print "if-cd-hit\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if-cd-miss\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
print "if-false-miss\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "if-true-hit\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "if-test-hit\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "if-test-miss\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ((((sub { open(my $__gh, '<', 'hay.txt') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'alpha') >= 0; } $__gn })->() > 0) && do {
    print "grep-and-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
})) {
    print "if-compound-hit\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    print "if-compound-miss\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
do { my $__ok = 1; for my $__d ('made') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
if ($? == 0) {
    __sh_uu_run('grep', '-q', 'alpha', 'hay.txt');$? = ($__sh_uu_rc) << 8; $CHILD_ERROR = $? >> 8;
    if ($? == 0) {
        print "mixed-chain-ran\n";
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
            } else {
$CHILD_ERROR = $? >> 8;
    }
} else {
$CHILD_ERROR = $? >> 8;
}
print("plain\n");
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "printf-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "echo-redirect-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir("/no/such/dir")) { print "never\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { warn "cd: " . "/no/such/dir" . ": $!\n"; $? = 1 << 8; $CHILD_ERROR = $? >> 8; }
say "compound-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('redir/sub') { my $__acc = $__d =~ m{^/} ? '' : '.'; for my $__c (split m{/+}, $__d) { next if $__c eq '' || $__c eq '.'; $__acc .= "/$__c"; next if -d $__acc; unless (mkdir($__acc)) { $__ok = 0; print STDERR "mkdir: $!\n"; last; } } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
{
    open(my $__sh2_dup0, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh0, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("redir")) {
        
    } else {
        warn "cd: " . "redir" . ": $!\n";
        
    }
    open(STDERR, '>&', $__sh2_dupfh0) or die; close($__sh2_dupfh0);
    open(STDERR, '>&', $__sh2_dup0) or die;
}
print("redir-pwd=" . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
{
    open(my $__sh2_dup1, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh1, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh1) or die; close($__sh2_dupfh1);
    open(STDERR, '>&', $__sh2_dup1) or die;
}
say "redir-cd-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir("..")) {
    
} else {
    warn "cd: " . ".." . ": $!\n";
    
}
do { require File::Path; File::Path::remove_tree('redir', 'made'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
do { my $__ok = 1; for my $__d ('redir/sub') { my $__acc = $__d =~ m{^/} ? '' : '.'; for my $__c (split m{/+}, $__d) { next if $__c eq '' || $__c eq '.'; $__acc .= "/$__c"; next if -d $__acc; unless (mkdir($__acc)) { $__ok = 0; print STDERR "mkdir: $!\n"; last; } } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
{
    open(my $__sh2_dup2, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh2, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("redir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "redir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh2) or die; close($__sh2_dupfh2);
    open(STDERR, '>&', $__sh2_dup2) or die;
}
if ($? == 0) {
    print("redir-and=" . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
    } else {
$CHILD_ERROR = $? >> 8;
}
if (chdir("..")) {
    
} else {
    warn "cd: " . ".." . ": $!\n";
    
}
if (do {
open(my $__sh2_duperr3, '>&', \*STDERR) or die; open(my $__sh2_dupferr3, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    if (chdir("redir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "redir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
open(STDERR, '>&', $__sh2_dupferr3) or die; close($__sh2_dupferr3); open(STDERR, '>&', $__sh2_duperr3) or die; 
($CHILD_ERROR == 0)
}) {
    print("redir-if=" . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
    }
if (chdir("..")) {
    
} else {
    warn "cd: " . ".." . ": $!\n";
    
}
do { require File::Path; File::Path::remove_tree('redir'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
{
open(my $__sh2_duperr4, '>&', \*STDERR) or die; open(my $__sh2_dupferr4, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__d ('redir-chain') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr4) or die; close($__sh2_dupferr4); open(STDERR, '>&', $__sh2_duperr4) or die; 
}
if ($CHILD_ERROR != 0) {
    print "redir-chain-failed\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "redir-chain-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr5, '>&', \*STDERR) or die; open(my $__sh2_dupferr5, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__d ('redir-chain') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr5) or die; close($__sh2_dupferr5); open(STDERR, '>&', $__sh2_duperr5) or die; 
}
if ($CHILD_ERROR != 0) {
    print "redir-chain-second-failed\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "redir-chain-rc2=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { require File::Path; File::Path::remove_tree('redir-chain'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
{
open(my $__sh2_duperr6, '>&', \*STDERR) or die; open(my $__sh2_dupferr6, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__t ('/no/such/f') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr6) or die; close($__sh2_dupferr6); open(STDERR, '>&', $__sh2_duperr6) or die; 
}
if ($CHILD_ERROR != 0) {
    print "redir-touch-failed\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "redir-touch-rc=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$x = do { __sh_pipe_capture(q{'cd' '/no/such/dir'; 'echo' $?}, "pipefail") };
say "captured-cd-rc=$x";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$y = do { __sh_pipe_capture(q{'cd' '.'; 'echo' $?}, "pipefail") };
say "captured-cd-ok=$y";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup7, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh7, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh7) or die; close($__sh2_dupfh7);
    open(STDERR, '>&', $__sh2_dup7) or die;
}
if ($? == 0) {
    print "cb-cd-and-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-cd-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir(".")) { print "cb-cd-and-ok\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { warn "cd: " . "." . ": $!\n"; $? = 1 << 8; $CHILD_ERROR = $? >> 8; }
say "cb-cd-and-ok=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr8, '>&', \*STDERR) or die; open(my $__sh2_dupferr8, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }

open(STDERR, '>&', $__sh2_dupferr8) or die; close($__sh2_dupferr8); open(STDERR, '>&', $__sh2_duperr8) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-cd-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-cd-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (((do {
open(my $__sh2_duperr9, '>&', \*STDERR) or die; open(my $__sh2_dupferr9, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
open(STDERR, '>&', $__sh2_dupferr9) or die; close($__sh2_dupferr9); open(STDERR, '>&', $__sh2_duperr9) or die; 
($CHILD_ERROR == 0)
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) == 0) {
    print "cb-bang-cd\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bang-cd=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (((do {
    if (chdir(".")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "." . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    ($? >> 8) == 0
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) == 0) {
    print "cb-bang-cd-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bang-cd-ran=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__t ('cb-file') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { unlink('cb-file'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
if ($? == 0) {
    print "cb-rm-and-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-rm-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('cb-dir') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
{
    open(my $__sh2_dup10, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh10, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__p ('cb-dir') { unless (unlink($__p)) { $__ok = 0; print STDERR "rm: cannot remove '$__p': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh10) or die; close($__sh2_dupfh10);
    open(STDERR, '>&', $__sh2_dup10) or die;
}
if ($? == 0) {
    print "cb-rm-dir-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-rm-dir-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr11, '>&', \*STDERR) or die; open(my $__sh2_dupferr11, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__p ('cb-dir') { unless (unlink($__p)) { $__ok = 0; print STDERR "rm: cannot remove '$__p': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr11) or die; close($__sh2_dupferr11); open(STDERR, '>&', $__sh2_duperr11) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-rm-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-rm-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('cb-dir') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { my $__ok = 1; for my $__d ('cb-rd') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { my $__ok = 1; for my $__d ('cb-rd') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
if ($? == 0) {
    print "cb-rmdir-and-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-rmdir-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup12, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh12, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('cb-rd') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh12) or die; close($__sh2_dupfh12);
    open(STDERR, '>&', $__sh2_dup12) or die;
}
if ($? == 0) {
    print "cb-rmdir-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-rmdir-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr13, '>&', \*STDERR) or die; open(my $__sh2_dupferr13, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__d ('cb-rd') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr13) or die; close($__sh2_dupferr13); open(STDERR, '>&', $__sh2_duperr13) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-rmdir-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-rmdir-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('cb-md') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
if ($? == 0) {
    print "cb-mkdir-and-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-mkdir-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup14, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh14, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('cb-md') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh14) or die; close($__sh2_dupfh14);
    open(STDERR, '>&', $__sh2_dup14) or die;
}
if ($? == 0) {
    print "cb-mkdir-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-mkdir-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr15, '>&', \*STDERR) or die; open(my $__sh2_dupferr15, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__d ('cb-md') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr15) or die; close($__sh2_dupferr15); open(STDERR, '>&', $__sh2_duperr15) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-mkdir-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-mkdir-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('cb-md') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { my $__ok = 1; for my $__t ('cb-t') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
if ($? == 0) {
    print "cb-touch-and-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-touch-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup16, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh16, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__t ('/no/such/t') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh16) or die; close($__sh2_dupfh16);
    open(STDERR, '>&', $__sh2_dup16) or die;
}
if ($? == 0) {
    print "cb-touch-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-touch-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr17, '>&', \*STDERR) or die; open(my $__sh2_dupferr17, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    do { my $__ok = 1; for my $__t ('/no/such/t') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };

open(STDERR, '>&', $__sh2_dupferr17) or die; close($__sh2_dupferr17); open(STDERR, '>&', $__sh2_duperr17) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-touch-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-touch-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { unlink('cb-t'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
print "cb-a\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "cb-echo-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-echo-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
print "cb-b\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "cb-echo-or-BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-echo-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print("cb-p\n");
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "cb-printf-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-printf-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup18, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh18, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    my $__sh2_pf_bad0 = 0;
    printf("%d\n", (do {
    my ($__pf_s, $__pf_v, $__pf_bad, $__pf_msg) = ("zz", 0, 0, 'invalid number');
    my $__pf_t = $__pf_s;
    $__pf_t =~ s/\A[ \t\n]+//;
    if ($__pf_t =~ /\A(?<sg>[+-]?)0[xX](?<dg>[0-9a-fA-F]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? hex($+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest}) || !length($+{dg})) { $__pf_bad = 1; $__pf_msg = 'invalid hex number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0[bB](?<dg>[01]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0b' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest}) || !length($+{dg});
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0(?<dg>[0-7]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest})) { $__pf_bad = 1; $__pf_msg = 'invalid octal number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)(?<dg>[0-9]+)(?<rest>.*)\z/s) {
        $__pf_v = 0 + $+{dg};
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest});
    } else {
        $__pf_bad = 1;
    }
    if ($__pf_bad) { print STDERR "printf: $__pf_s: $__pf_msg\n"; $__sh2_pf_bad0 = 1; }
    $__pf_v
}));
    $? = (($__sh2_pf_bad0 ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
    open(STDERR, '>&', $__sh2_dupfh18) or die; close($__sh2_dupfh18);
    open(STDERR, '>&', $__sh2_dup18) or die;
}
if ($? == 0) {
    print "cb-printf-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-printf-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
open(my $__sh2_duperr19, '>&', \*STDERR) or die; open(my $__sh2_dupferr19, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    my $__sh2_pf_bad1 = 0;
    printf("%d\n", (do {
    my ($__pf_s, $__pf_v, $__pf_bad, $__pf_msg) = ("zz", 0, 0, 'invalid number');
    my $__pf_t = $__pf_s;
    $__pf_t =~ s/\A[ \t\n]+//;
    if ($__pf_t =~ /\A(?<sg>[+-]?)0[xX](?<dg>[0-9a-fA-F]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? hex($+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest}) || !length($+{dg})) { $__pf_bad = 1; $__pf_msg = 'invalid hex number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0[bB](?<dg>[01]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0b' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest}) || !length($+{dg});
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0(?<dg>[0-7]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest})) { $__pf_bad = 1; $__pf_msg = 'invalid octal number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)(?<dg>[0-9]+)(?<rest>.*)\z/s) {
        $__pf_v = 0 + $+{dg};
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest});
    } else {
        $__pf_bad = 1;
    }
    if ($__pf_bad) { print STDERR "printf: $__pf_s: $__pf_msg\n"; $__sh2_pf_bad1 = 1; }
    $__pf_v
}));
    $? = (($__sh2_pf_bad1 ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;

open(STDERR, '>&', $__sh2_dupferr19) or die; close($__sh2_dupferr19); open(STDERR, '>&', $__sh2_duperr19) or die; 
}
if ($CHILD_ERROR != 0) {
    print "cb-printf-or-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;

}
say "cb-printf-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "cb-true-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-true-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "cb-false-and-BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-false-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "cb-false-or\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-false-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "cb-true-or-BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-true-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (((do {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) != 0) {
    print "cb-bang-true-or\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bang-true-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (((do {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) == 0) {
    print "cb-bang-false-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bang-false-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$CHILD_ERROR = (-f '/etc/passwd') ? 0 : 1;
if (-f '/etc/passwd') {
    print "cb-bracket-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bracket-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$CHILD_ERROR = (-f '/no/passwd') ? 0 : 1;
if (-f '/no/passwd') {
    print "cb-bracket-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bracket-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (!(-f '/no/passwd')) {
    print "cb-bracket-or\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bracket-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ((-d '/tmp' && -f '/etc/passwd')) {
    print "cb-bracket-chain\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bracket-chain=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$CHILD_ERROR = (-d '/tmp') ? 0 : 1;
if (-d '/tmp') {
    print "cb-dbl-and\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-dbl-and=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$CHILD_ERROR = (-f '/no/x') ? 0 : 1;
if (-f '/no/x') {
    print "cb-dbl-ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-dbl-fail=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (!(-f '/no/x')) {
    print "cb-dbl-or\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-dbl-or=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ((-d '/tmp' && -f '/etc/passwd')) {
    print "cb-dbl-both\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-dbl-both=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (!(-f '/no/x')) {
    print "cb-bang-dbl\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-bang-dbl=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup20, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh20, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh20) or die; close($__sh2_dupfh20);
    open(STDERR, '>&', $__sh2_dup20) or die;
}
if ($? == 0) {
    print "cb-never\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-later=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "cb-still-running\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if (chdir(".")) { print "cb-cd-after-false\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8; } else { warn "cd: " . "." . ": $!\n"; $? = 1 << 8; $CHILD_ERROR = $? >> 8; }
say "cb-cd-after-false=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "cb-true-after-false\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}
say "cb-true-after-false=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$CHILD_ERROR = (-f '/etc/passwd') ? 0 : 1;
if (-f '/etc/passwd') {
    print "cb-test-after-false\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cb-test-after-false=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;

exit $? >> 8;
