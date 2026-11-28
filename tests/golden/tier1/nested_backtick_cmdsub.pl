#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);

our $CHILD_ERROR = 0;
use feature 'say';

my $x;
my $y;
my $z;
my $w;
my $v;
my $q;
my $d;
my $K;
my $M;
my $N;
my $O;
my $S;
my $T;
my $W;
my $U;
my $P;

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

do { my $__ok = 1; for my $__d ('nested/sub') { my $__acc = $__d =~ m{^/} ? '' : '.'; for my $__c (split m{/+}, $__d) { next if $__c eq '' || $__c eq '.'; $__acc .= "/$__c"; next if -d $__acc; unless (mkdir($__acc)) { $__ok = 0; print STDERR "mkdir: $!\n"; last; } } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
open(my $__ofh0, '>', 'nested/sub/data.txt') or die "echo: 'nested/sub/data.txt': $ERRNO"; print $__ofh0 q{file body
}; close $__ofh0; $? = 0 << 8; $CHILD_ERROR = $? >> 8;$x = do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
print("x=" . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename($x) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
$y = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
say "y=$y";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$z = do { __sh_pipe_capture(q{'cd' 'nested' && 'basename' $('pwd')}, "pipefail") };
say "z=$z";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$w = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, do { __sh_pipe_capture(q{'basename' $('pwd')}, "pipefail") }));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "w=$w";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$v = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
say "v=$v";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$q = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
say "q=$q";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$d = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
$K = (do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, $d));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap }) . "-" . (do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, $d));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap });
$ENV{K} = ${K} if exists $ENV{K};
say "K=$K";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$M = "pre" . ("hi") . "post";
$ENV{M} = ${M} if exists $ENV{M};
say "M=$M";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$N = ("hi") . ("bye");
$ENV{N} = ${N} if exists $ENV{N};
say "N=$N";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$O = (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }) . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap });
$ENV{O} = ${O} if exists $ENV{O};
say "O=$O";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$S = (do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, $d));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap }) . "/" . (do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, $d));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap });
$ENV{S} = ${S} if exists $ENV{S};
say "S=$S";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$T = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say join(' ', grep { defined($_) && length($_) } split(/\s+/, do { __sh_pipe_capture(q{'basename' $('pwd')}, "pipefail") }), split(/\s+/, do { __sh_pipe_capture(q{'basename' $('pwd')}, "pipefail") }), split(/\s+/, do { __sh_pipe_capture(q{'basename' $('pwd')}, "pipefail") }));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "T=$T";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$W = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
say "W=$W";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$U = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print((do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }) . ":" . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "U=$U";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "R=ok";
$P = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap }) }); $? = 0 << 8; $CHILD_ERROR = $? >> 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
say "rc-after-false=$CHILD_ERROR";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "P=$P";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;

exit $? >> 8;
