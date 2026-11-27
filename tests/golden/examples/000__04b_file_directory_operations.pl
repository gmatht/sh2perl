#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);

our $CHILD_ERROR = 0;
use feature 'say';

my $d;
my $file_list;
my $found_files;

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
sub __sh_uu_capture {
    my @argv = @_;
    if ($__SH_UU && ($__SH_UU_OK{ $argv[0] // '' } // 0)) {
        # Capture = fd-1 redirect to a temp file around the IN-PROCESS
        # uu_run call (the fd redirect is what the C wrapper's drain
        # thread exists for; a temp file cannot fill, so no deadlock).
        my $tmpf = "/tmp/sh2_uu_${$}_" . ($__SH_UU_SEQ++) . ".cap";
        open(my $saved, '>&', \*STDOUT) or do { $__sh_uu_rc = 127; return ''; };
        open(STDOUT, '>', $tmpf) or do { $__sh_uu_rc = 127; return ''; };
        my $rc = eval { __sh_uu_run_ffi($argv[0], scalar(@argv), \@argv) };
        open(STDOUT, '>&', $saved) or do { $__sh_uu_rc = 127; return ''; };
        close($saved);
        $__sh_uu_rc = defined $rc ? $rc : 127;
        my $out = '';
        if (open(my $rf, '<', $tmpf)) { $out = do { local $/; <$rf> } // ''; close $rf; }
        unlink $tmpf;
        return $out;
    }
    my $out = do {
        open(my $fh, '-|', @argv) or do { $__sh_uu_rc = 127; return ''; };
        my $r = do { local $/; <$fh> };
        close $fh;
        $r;
    };
    $__sh_uu_rc = $? >> 8;
    return $out // '';
}
# __SH_RUN_EXT_END

print "=== File and Directory Operations ===\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$d = do { my $__sh_cap; $__sh_cap = do { require File::Temp; File::Temp::tempdir('sh2_ps_XXXXXX', TMPDIR => 1, CLEANUP => 0) }; $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
chdir($d) or do { warn "cd: " . $d . ": $!\n"; exit "1"; };
open(my $__ofh0, '>', 'a.sh') or die "echo: 'a.sh': $ERRNO"; print $__ofh0 q{a
}; close $__ofh0; $? = 0 << 8; $CHILD_ERROR = $? >> 8;open(my $__ofh1, '>', 'b.txt') or die "echo: 'b.txt': $ERRNO"; print $__ofh1 q{b
}; close $__ofh1; $? = 0 << 8; $CHILD_ERROR = $? >> 8;do { my $__ok = 1; for my $__d ('sub') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
open(my $__ofh2, '>', 'sub/c.sh') or die "echo: 'sub/c.sh': $ERRNO"; print $__ofh2 q{c
}; close $__ofh2; $? = 0 << 8; $CHILD_ERROR = $? >> 8;open(my $__ofh3, '>', '.hidden') or die "echo: '.hidden': $ERRNO"; print $__ofh3 q{x
}; close $__ofh3; $? = 0 << 8; $CHILD_ERROR = $? >> 8;$file_list = do { my $_r = __sh_uu_capture('ls', '-A'); $CHILD_ERROR = $__sh_uu_rc; $_r =~ s/\n+\z//; $_r; };
print "File listing:\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say $file_list;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$found_files = do { my $_r = __sh_uu_capture('find', '.', '-name', '*.sh', '-type', 'f'); $CHILD_ERROR = $__sh_uu_rc; $_r =~ s/\n+\z//; $_r; };
print "Found shell scripts:\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say $found_files;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir("/")) {
    
} else {
    warn "cd: " . "/" . ": $!\n";
    
}
do { require File::Path; File::Path::remove_tree($d); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
print "=== File and Directory Operations Complete ===\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;

exit $? >> 8;
