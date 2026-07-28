#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $LIBC;

my $timestamp = q{2022-01-09};
my $me = do { my $result_0 = qx{bash -c q{echo "$0" | sed -e 's,.*/,,'} }; chomp $result_0; $result_0; };
my $usage = "\\\nUsage: $PROGRAM_NAME [OPTION]\n\nOutput the configuration name of the " . "sys" . "tem" . " \\" . chr(96) . "$me' is run on.

Options:
  -h, --help         print this help, then exit
  -t, --time-stamp   print date of last modification, then exit
  -v, --version      print version number, then exit

Report bugs and patches to <config-patches@gnu.org>.";
my $version = "\
GNU config.guess ($timestamp)

Originally written by Per Bothner.
Copyright 1992-2022 Free Software Foundation, Inc.

This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.";
my $help = "
Try \\" . chr(96) . "$me --help' for more information.";
my $# = 0;
while ( (Variable("#", false, None) > 0) ) {
if ($arg1 eq '--time-stamp' or $arg1 =~ /^--time.*$/msx or $arg1 eq '-t') {
                say $timestamp;
        exit $main_exit_code;
    } elsif ($arg1 eq '--version' or $arg1 eq '-v') {
                say $version;
        exit $main_exit_code;
    } elsif ($arg1 eq '--help' or $arg1 =~ /^--h.*$/msx or $arg1 eq '-h') {
                say $usage;
        exit $main_exit_code;
    } elsif ($arg1 eq '--') {
        # Builtin command 'shift' not implemented
        last;    } elsif ($arg1 eq '-') {
        last;    } elsif ($arg1 =~ /^-.*$/msx) {
                do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
            say "$me: invalid option $_[0]$help";
        };
        exit 1;
    } elsif (1) {
        last;    }
}
if ((!Variable("#", false, None) eq 0)) {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "$me: too many arguments$help";
    };
exit 1;
}
my $GUESS = q{};
my $tmp = q{};
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx'test -z "$tmp" || rm -fr "$tmp" 2>&1'; print $end_out if $end_out ne q{}; }

sub set_cc_for_build {
    if (do {
$main_exit_code = system('test', "$tmp") >> 8;
        $CHILD_ERROR == 0
    }) {
        return q{0};
    }
    $main_exit_code = system(':', ($ENV{TMPDIR=/tmp} // q{})) >> 8;
                    if (do {
if (do {
$tmp = do { my @_qx_cmd = ("(umask 077 && mktemp -d \"$TMPDIR/cgXXXXXX\") 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $CHILD_ERROR == 0
}) {
        $main_exit_code = system('test', '-n', "$tmp") >> 8;
}
            $CHILD_ERROR == 0
        }) {
                        $main_exit_code = system('test', '-d', "$tmp") >> 8;
        }
    if ($CHILD_ERROR != 0) {
                    if (do {
if (do {
$main_exit_code = system('test', '-n', "$ENV{RANDOM}") >> 8;
    $CHILD_ERROR == 0
}) {
        $tmp = $TMPDIR;
    $main_exit_code = system('/cg', $$, q{-}, $RANDOM) >> 8;
}
                $CHILD_ERROR == 0
            }) {
                                do {
                    local %ENV = %ENV;
                    my $me = $me;
                    my $help = $help;
                    my $GUESS = $GUESS;
                    my $timestamp = $timestamp;
                    my $usage = $usage;
                    my $version = $version;
                    my $# = $#;
                    my $tmp = $tmp;
                    if (do {
$main_exit_code = system('umask', q{077}) >> 8;
                        $CHILD_ERROR == 0
                    }) {
                                                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
                            use File::Path qw(make_path);
                            my $err;
                            if ( mkdir "$tmp" ) {
                                }
                            else {
                                croak "mkdir: cannot create directory " . "$tmp" . ": File exists\n";
                            }
                        };
                    }
                    q{};
                };
            }
    }
    if ($CHILD_ERROR != 0) {
                    $tmp = $TMPDIR;
            if (do {
if (do {
$main_exit_code = system('/cg-', $$) >> 8;
    $CHILD_ERROR == 0
}) {
        do {
        local %ENV = %ENV;
        my $err = $err;
        my $me = $me;
        my $help = $help;
        my $GUESS = $GUESS;
        my $timestamp = $timestamp;
        my $usage = $usage;
        my $version = $version;
        my $# = $#;
        my $tmp = $tmp;
        if (do {
$main_exit_code = system('umask', q{077}) >> 8;
            $CHILD_ERROR == 0
        }) {
                        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
                use File::Path qw(make_path);
                if ( mkdir "$tmp" ) {
                    }
                else {
                    croak "mkdir: cannot create directory " . "$tmp" . ": File exists\n";
                }
            };
        }
        q{};
    };
}
                $CHILD_ERROR == 0
            }) {
                                do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
                    say "Warning: creating insecure temp directory";
                };
            }
    }
    if ($CHILD_ERROR != 0) {
                    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
                say "$me: cannot create a temporary directory in $ENV{TMPDIR}";
            };
exit 1;
    }
    my $dummy = $tmp;
    $main_exit_code = system('bash', '/dummy') >> 8;
    my $CC_FOR_BUILD;
if ("$ENV{CC_FOR_BUILD-},$ENV{HOST_CC-},$ENV{CC-}" eq ',,') {
                open my $fh, '>', "\"\$dummy.c\"" or die ""$dummy.c": $!\n";
        say {*fh} "int x;";
        close $fh;
                my $driver;
        for my $driver ('cc', 'gcc', 'c89', 'c99') {
if (!(            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
                do {
                    local %ENV = %ENV;
                    my $err = $err;
                    my $CC_FOR_BUILD = $CC_FOR_BUILD;
                    my $dummy = $dummy;
                    my $driver = $driver;
                    my $me = $me;
                    my $help = $help;
                    my $GUESS = $GUESS;
                    my $timestamp = $timestamp;
                    my $usage = $usage;
                    my $version = $version;
                    my $# = $#;
                    my $tmp = $tmp;
                    $CHILD_ERROR = 0;
                    q{};
                };
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };)) {
                $CC_FOR_BUILD = $driver;
last;
            }
        }
        if (x StringInterpolation(StringInterpolation { parts: [Variable("CC_FOR_BUILD")] }, None) eq x) {
            $CC_FOR_BUILD = 'no_compiler_found';
        }
    } elsif ("$ENV{CC_FOR_BUILD-},$ENV{HOST_CC-},$ENV{CC-}" =~ /^,,.*$/msx) {
                $CC_FOR_BUILD = $CC;
    } elsif ("$ENV{CC_FOR_BUILD-},$ENV{HOST_CC-},$ENV{CC-}" =~ /^,.*,.*$/msx) {
                $CC_FOR_BUILD = $HOST_CC;
    }
;
    return;
}
my $PATH;
if ((-f '/.attbin/uname')) {
    $PATH = $PATH;
    $main_exit_code = system('bash', ':/.attbin') >> 8;
$ENV{PATH} = $PATH;
}
my $UNAME_MACHINE = do { my @_qx_cmd = ("uname -m 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ($CHILD_ERROR != 0) {
        $UNAME_MACHINE = 'unknown';
}
my $UNAME_RELEASE = do { my @_qx_cmd = ("uname -r 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ($CHILD_ERROR != 0) {
        $UNAME_RELEASE = 'unknown';
}
my $UNAME_SYSTEM = do { my @_qx_cmd = ("uname -s 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ($CHILD_ERROR != 0) {
        $UNAME_SYSTEM = 'unknown';
}
my $UNAME_VERSION = do { my @_qx_cmd = ("uname -v 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ($CHILD_ERROR != 0) {
        $UNAME_VERSION = 'unknown';
}
my $cc_set_libc;
if ($UNAME_SYSTEM eq 'Linux' or $UNAME_SYSTEM eq 'GNU' or $UNAME_SYSTEM =~ /^GNU/.*$/msx) {
        $LIBC = 'unknown';
        set_cc_for_build();
    open my $fh_cat, '>', "\"\$ENV{dummy}.c\"" or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "\t#include <features.h>
\t#if defined(__UCLIBC__)
\tLIBC=uclibc
\t#elif defined(__dietlibc__)
\tLIBC=dietlibc
\t#elif defined(__GLIBC__)
\tLIBC=gnu
\t#else
\t#include <stdarg.h>
\t/* First heuristic to detect musl libc.  */
\t#ifdef __DEFINED_va_list
\tLIBC=musl
\t#endif
\t#endif
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
        $cc_set_libc = do { my $result_3 = qx{bash -c q{$CC_FOR_BUILD -E "$dummy.c" 2> /dev/null | grep ^LIBC | sed 's, ,,g'} }; chomp $result_3; $result_3; };
    do { my $eval_input = $cc_set_libc; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    if ((("$LIBC" eq unknown && !(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'ldd') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    })) && !(do {
        my $output_4 = q{};
        my $output_printed_4;
        my $pipeline_success_4 = 1;
                $output = q{};
                do {
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $tmp_redirect_5 = q{};

my $cmd_8 = 'ldd';
my ($in_7, $out_7);
my $pid_7 = open3($in_7, $out_7, '>&STDERR', $cmd_8, '--version');
print {$in_7} $output_4;
close $in_7 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_5 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_7> };
close $out_7 or croak 'Close failed: $OS_ERROR';
waitpid $pid_7, 0;
$tmp_redirect_5;
        };
        $output_4 = $output;

                my $grep_result_4_1;
        my @grep_lines_4_1 = split /\n/msx, $output_4;
        my @grep_filtered_4_1 = grep { /^musl/msx } @grep_lines_4_1;
        $grep_result_4_1 = join "\n", @grep_filtered_4_1;
        if (!($grep_result_4_1 =~ m{\n\z} || $grep_result_4_1 eq q{})) {
        $grep_result_4_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_4_1 > 0 ? 0 : 1;
        $grep_result_4_1 = q{};
        $output_4 = q{};
        if ((scalar @grep_filtered_4_1) == 0) {
            $pipeline_success_4 = 0;
        }
        if ($output_4 ne q{} && !defined $output_printed_4) {
            print $output_4;
            if (!($output_4 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
        }))) {
        $LIBC = 'musl';
    }
    if ("$LIBC" eq unknown) {
        $LIBC = 'gnu';
    }
}
my $SUN_REL;
my $OSF_REL;
my $machine;
my $release;
my $HP_ARCH;
my $GNU_REL;
my $abi;
my $HPUX_REV;
my $SUN_ARCH;
my $expr;
my $IRIX_REL;
my $arch;
my $CCOPTS;
my $FUJITSU_SYS;
my $LIBCABI;
my $UNAME_REL;
my $UNAME_PROCESSOR;
my $sc_cpu_version;
my $SYSTEM_NAME;
my $GNU_ARCH;
my $FREEBSD_REL;
my $cc_set_vars;
my $IBM_REV;
my $CC_FOR_BUILD;
my $ALPHA_CPU_TYPE;
my $sc_kernel_bits;
my $os;
my $endian;
my $UNAME_MACHINE_ARCH;
my $dummyarg;
my $IBM_CPU_ID;
my $CRAY_REL;
my $GNU_SYS;
my $FUJITSU_PROC;
my $IBM_ARCH;
my $IS_GLIBC;
my $OS_REL;
my $DRAGONFLY_REL;
my $FUJITSU_REL;
my $SKYOS_REL;
if ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:NetBSD:.*:.*$/msx) {
        $UNAME_MACHINE_ARCH = do {
    my $command = '(uname -p 2> /dev/null || /sbin/sysctl -n hw.machine_arch 2> /dev/null || /usr/sbin/sysctl -n hw.machine_arch 2> /dev/null || echo unknown)';
    my ($in, $out, $err);
    my $pid = open3($in, $out, $err, 'bash', '-c', $command);
    close $in or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result;
};
    if ($UNAME_MACHINE_ARCH eq 'aarch64eb') {
                $machine = 'aarch64_be-unknown';
    } elsif ($UNAME_MACHINE_ARCH eq 'armeb') {
                $machine = 'armeb-unknown';
    } elsif ($UNAME_MACHINE_ARCH =~ /^arm.*$/msx) {
                $machine = 'arm-unknown';
    } elsif ($UNAME_MACHINE_ARCH eq 'sh3el') {
                $machine = 'shl-unknown';
    } elsif ($UNAME_MACHINE_ARCH eq 'sh3eb') {
                $machine = 'sh-unknown';
    } elsif ($UNAME_MACHINE_ARCH eq 'sh5el') {
                $machine = 'sh5le-unknown';
    } elsif ($UNAME_MACHINE_ARCH =~ /^earmv.*$/msx) {
                $arch = do { my $result_9 = qx{bash -c 'echo "$UNAME_MACHINE_ARCH" | sed -e "s,^e\\\\(armv[0-9]\\\\).*\\$,\\\\1,"' }; chomp $result_9; $result_9; };
                $endian = do { my $result_10 = qx{bash -c 'echo "$UNAME_MACHINE_ARCH" | sed -ne "s,^.*\\\\(eb\\\\)\\$,\\\\1,p"' }; chomp $result_10; $result_10; };
                $machine = $arch;
                $CHILD_ERROR = 0;
    } elsif (1) {
                $machine = $UNAME_MACHINE_ARCH;
                $main_exit_code = system('bash', '-unknown') >> 8;
    }
    if ($UNAME_MACHINE_ARCH =~ /^earm.*$/msx) {
                $os = 'netbsdelf';
    } elsif ($UNAME_MACHINE_ARCH =~ /^arm.*$/msx or $UNAME_MACHINE_ARCH eq 'i386' or $UNAME_MACHINE_ARCH eq 'm68k' or $UNAME_MACHINE_ARCH eq 'ns32k' or $UNAME_MACHINE_ARCH =~ /^sh3.*$/msx or $UNAME_MACHINE_ARCH eq 'sparc' or $UNAME_MACHINE_ARCH eq 'vax') {
                set_cc_for_build();
        if (!(        # Original bash: echo __ELF__ | $CC_FOR_BUILD -E - 2>/dev/null \
do {
            my $output_11 = q{};
            my $output_printed_11;
            my $pipeline_success_11 = 1;
            $output_11 .= '__ELF__' . "\n";
if ( !($output_11 =~ m{\n\z}) ) { $output_11 .= "\n"; }

                        my $cmd_13 = 'unknown_command';
            my ($in_12, $out_12);
            my $pid_12 = open3($in_12, $out_12, '>&STDERR', $cmd_13, '-E', q{-});
            print {$in_12} $output_11;
            close $in_12 or croak 'Close failed: $OS_ERROR';
            $output_11 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_12> };
            close $out_12 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_12, 0;

                        my $grep_result_11_2;
            my @grep_lines_11_2 = split /\n/msx, $output_11;
            my @grep_filtered_11_2 = grep { /__ELF__/msx } @grep_lines_11_2;
            $grep_result_11_2 = join "\n", @grep_filtered_11_2;
            if (!($grep_result_11_2 =~ m{\n\z} || $grep_result_11_2 eq q{})) {
            $grep_result_11_2 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_11_2 > 0 ? 0 : 1;
            $grep_result_11_2 = q{};
            $output_11 = q{};
            if ((scalar @grep_filtered_11_2) == 0) {
                $pipeline_success_11 = 0;
            }
            if ($output_11 ne q{} && !defined $output_printed_11) {
                print $output_11;
                if (!($output_11 =~ m{\n\z})) {
                    print "\n";
                }
            }
            if ( !$pipeline_success_11 ) { $main_exit_code = 1; }
            };)) {
            $os = 'netbsd';
}
        else {
            $os = 'netbsdelf';
        }
    } elsif (1) {
                $os = 'netbsd';
    }
    if ($UNAME_MACHINE_ARCH =~ /^earm.*$/msx) {
                $expr = 's/^earmv[0-9]/-eabi/;s/eb$//';
                $abi = do { my $result_14 = qx{bash -c 'echo "$UNAME_MACHINE_ARCH" | sed -e "$expr"' }; chomp $result_14; $result_14; };
    }
    if ($UNAME_VERSION =~ /^Debian.*$/msx) {
                $release = '-gnu';
    } elsif (1) {
                $release = do { my $result_15 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-_].*//' | cut -d . -f 1,2} }; chomp $result_15; $result_15; };
    }
        $GUESS = $machine;
        $main_exit_code = system('-', $os, $release, $abi-) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Bitrig:.*:.*$/msx) {
        $UNAME_MACHINE_ARCH = do { my $result_16 = qx{bash -c 'arch | sed s/Bitrig.//' }; chomp $result_16; $result_16; };
        $GUESS = $UNAME_MACHINE_ARCH;
        $main_exit_code = system('-unknown-bitrig', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:OpenBSD:.*:.*$/msx) {
        $UNAME_MACHINE_ARCH = do { my $result_17 = qx{bash -c 'arch | sed s/OpenBSD.//' }; chomp $result_17; $result_17; };
        $GUESS = $UNAME_MACHINE_ARCH;
        $main_exit_code = system('-unknown-openbsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:SecBSD:.*:.*$/msx) {
        $UNAME_MACHINE_ARCH = do { my $result_18 = qx{bash -c 'arch | sed s/SecBSD.//' }; chomp $result_18; $result_18; };
        $GUESS = $UNAME_MACHINE_ARCH;
        $main_exit_code = system('-unknown-secbsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:LibertyBSD:.*:.*$/msx) {
        $UNAME_MACHINE_ARCH = do { my $result_19 = qx{bash -c 'arch | sed "s/^.*BSD\\\\.//"' }; chomp $result_19; $result_19; };
        $GUESS = $UNAME_MACHINE_ARCH;
        $main_exit_code = system('-unknown-libertybsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:MidnightBSD:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-midnightbsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:ekkoBSD:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-ekkobsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:SolidBSD:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-solidbsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:OS108:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-os108_', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^macppc:MirBSD:.*:.*$/msx) {
        $GUESS = 'powerpc-unknown-mirbsd';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:MirBSD:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-mirbsd', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Sortix:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-sortix') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Twizzler:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-twizzler') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Redox:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-redox') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips:OSF1:.*..*$/msx) {
        $GUESS = 'mips-dec-osf1';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^alpha:OSF1:.*:.*$/msx) {
    END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx' 2>&1'; print $end_out if $end_out ne q{}; }
    if ($UNAME_RELEASE =~ /^.*4.0$/msx) {
                $UNAME_RELEASE = do { my $result_20 = qx{bash -c q(/usr/sbin/sizer -v | awk '{print $3}') }; chomp $result_20; $result_20; };
    } elsif ($UNAME_RELEASE =~ /^.*5..*$/msx) {
                $UNAME_RELEASE = do { my $result_21 = qx{bash -c q(/usr/sbin/sizer -v | awk '{print $4}') }; chomp $result_21; $result_21; };
    }
        $ALPHA_CPU_TYPE = do { my $result_22 = qx{bash -c '/usr/sbin/psrinfo -v | sed -n -e "s/^  The alpha \\\\(.*\\\\) processor.*\\$/\\\\1/p" | head -n 1' }; chomp $result_22; $result_22; };
    if ($ALPHA_CPU_TYPE eq 'EV4 (21064)') {
                $UNAME_MACHINE = 'alpha';
    } elsif ($ALPHA_CPU_TYPE eq 'EV4.5 (21064)') {
                $UNAME_MACHINE = 'alpha';
    } elsif ($ALPHA_CPU_TYPE eq 'LCA4 (21066/21068)') {
                $UNAME_MACHINE = 'alpha';
    } elsif ($ALPHA_CPU_TYPE eq 'EV5 (21164)') {
                $UNAME_MACHINE = 'alphaev5';
    } elsif ($ALPHA_CPU_TYPE eq 'EV5.6 (21164A)') {
                $UNAME_MACHINE = 'alphaev56';
    } elsif ($ALPHA_CPU_TYPE eq 'EV5.6 (21164PC)') {
                $UNAME_MACHINE = 'alphapca56';
    } elsif ($ALPHA_CPU_TYPE eq 'EV5.7 (21164PC)') {
                $UNAME_MACHINE = 'alphapca57';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6 (21264)') {
                $UNAME_MACHINE = 'alphaev6';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6.7 (21264A)') {
                $UNAME_MACHINE = 'alphaev67';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6.8CB (21264C)') {
                $UNAME_MACHINE = 'alphaev68';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6.8AL (21264B)') {
                $UNAME_MACHINE = 'alphaev68';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6.8CX (21264D)') {
                $UNAME_MACHINE = 'alphaev68';
    } elsif ($ALPHA_CPU_TYPE eq 'EV6.9A (21264/EV69A)') {
                $UNAME_MACHINE = 'alphaev69';
    } elsif ($ALPHA_CPU_TYPE eq 'EV7 (21364)') {
                $UNAME_MACHINE = 'alphaev7';
    } elsif ($ALPHA_CPU_TYPE eq 'EV7.9 (21364A)') {
                $UNAME_MACHINE = 'alphaev79';
    }
        $OSF_REL = do { my $result_23 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/^[PVTX]//' | tr ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz} }; chomp $result_23; $result_23; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-dec-osf', $OSF_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Amiga.*:UNIX_System_V:4.0:.*$/msx) {
        $GUESS = 'm68k-unknown-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:\[Aa\]miga\[Oo\]\[Ss\]:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-amigaos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:\[Mm\]orph\[Oo\]\[Ss\]:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-morphos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:OS/390:.*:.*$/msx) {
        $GUESS = 'i370-ibm-openedition';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:z/VM:.*:.*$/msx) {
        $GUESS = 's390-ibm-zvmoe';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:OS400:.*:.*$/msx) {
        $GUESS = 'powerpc-ibm-os400';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm:RISC.*:1.\[012\].*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm:riscix:1.\[012\].*:.*$/msx) {
        $GUESS = 'arm-acorn-riscix';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm.*:riscos:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm.*:RISCOS:.*:.*$/msx) {
        $GUESS = 'arm-unknown-riscos';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SR2.01:HI-UX/MPP:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SR8000:HI-UX/MPP:.*:.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-h', 'itachi-hiuxmpp') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Pyramid.*:OSx.*:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^MIS.*:OSx.*:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^MIS.*:SMP_DC-OSx.*:.*:.*$/msx) {
    if (do { my @_qx_cmd = ("/bin/universe 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'att') {
                $GUESS = 'pyramid-pyramid-sysv3';
    } elsif (1) {
                $GUESS = 'pyramid-pyramid-bsd';
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NILE.*:.*:.*:dcosx$/msx) {
        $GUESS = 'pyramid-pyramid-svr4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^DRS.6000:unix:4.0:6.*$/msx) {
        $GUESS = 'sparc-icl-nx6';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^DRS.6000:UNIX_SV:4.2.*:7.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^DRS.6000:isis:4.2.*:7.*$/msx) {
    if (do {
    my ($in_24, $out_24);
    my $pid_24 = open3($in_24, $out_24, '>&STDERR', '/usr/bin/uname', '-p');
    close $in_24 or croak 'Close failed: $OS_ERROR';
    my $result_24 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_24> };
    close $out_24 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_24, 0;
    $result_24
} eq 'sparc') {
                $GUESS = 'sparc-icl-nx7';
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^s390x:SunOS:.*:.*$/msx) {
        $SUN_REL = do { my $result_25 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_25; $result_25; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-ibm-solaris2', $SUN_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun4H:SunOS:5..*:.*$/msx) {
        $SUN_REL = do { my $result_26 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_26; $result_26; };
        $GUESS = 'sparc-hal-solaris2';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun4.*:SunOS:5..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^tadpole.*:SunOS:5..*:.*$/msx) {
        $SUN_REL = do { my $result_27 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_27; $result_27; };
        $GUESS = 'sparc-sun-solaris2';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i86pc:AuroraUX:5..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i86xen:AuroraUX:5..*:.*$/msx) {
        $GUESS = 'i386-pc-auroraux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i86pc:SunOS:5..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i86xen:SunOS:5..*:.*$/msx) {
        set_cc_for_build();
        $SUN_ARCH = 'i386';
    if ((!StringInterpolation(StringInterpolation { parts: [Variable("CC_FOR_BUILD")] }, None) eq no_compiler_found)) {
if (!(        # Original bash: #! /bin/sh
do {
            my $output_28 = q{};
            my $output_printed_28;
            my $pipeline_success_28 = 1;
                        $output_28 = q{};
            $output_28 .= '#ifdef __amd64' . "\n";
            if ( !($output_28 =~ m{\n\z}) ) { $output_28 .= "\n"; }
            $output_28 .= 'IS_64BIT_ARCH' . "\n";
            if ( !($output_28 =~ m{\n\z}) ) { $output_28 .= "\n"; }
            $output_28 .= '#endif' . "\n";
            if ( !($output_28 =~ m{\n\z}) ) { $output_28 .= "\n"; }

                        $output_28 = q{};
            my @_pcmd_30 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
            my ($in_29, $out_29);
            my $pid_29 = open3($in_29, $out_29, '>&STDERR', @_pcmd_30);
            close $in_29 or croak 'Close failed: $OS_ERROR';
            $output_28 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_29> };
            close $out_29 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_29, 0;
            my @_pcmd_32 = ('sh', '-c', '$CC_FOR_BUILD -m64 -E - 2> /dev/null');
            my ($in_31, $out_31);
            my $pid_31 = open3($in_31, $out_31, '>&STDERR', @_pcmd_32);
            close $in_31 or croak 'Close failed: $OS_ERROR';
            $output_28 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> };
            close $out_31 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_31, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            my $tmp_redirect_33 = q{};
            my $grep_result_34;
            my @grep_lines_34 = split /\n/msx, $output_28;
            my @grep_filtered_34 = grep { /IS_64BIT_ARCH/msx } @grep_lines_34;
            $grep_result_34 = join "\n", @grep_filtered_34;
            if (!($grep_result_34 =~ m{\n\z} || $grep_result_34 eq q{})) {
            $grep_result_34 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_34 > 0 ? 0 : 1;
            $tmp_redirect_33 = $grep_result_34;
            $tmp_redirect_33;
            };
            print $tmp;
            if ($tmp eq q{}) { print $output_28; }
            $output_printed_28 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_28 ) { $main_exit_code = 1; }
            };)) {
            $SUN_ARCH = 'x86_64';
        }
    }
        $SUN_REL = do { my $result_35 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_35; $result_35; };
        $GUESS = $SUN_ARCH;
        $main_exit_code = system('-pc-solaris2', $SUN_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun4.*:SunOS:6.*:.*$/msx) {
        $SUN_REL = do { my $result_36 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_36; $result_36; };
        $GUESS = 'sparc-sun-solaris3';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun4.*:SunOS:.*:.*$/msx) {
    if (do {
    my ($in_37, $out_37);
    my $pid_37 = open3($in_37, $out_37, '>&STDERR', '/usr/bin/arch', '-k');
    close $in_37 or croak 'Close failed: $OS_ERROR';
    my $result_37 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_37> };
    close $out_37 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_37, 0;
    $result_37
} =~ /^Series.*$/msx or do {
    my ($in_38, $out_38);
    my $pid_38 = open3($in_38, $out_38, '>&STDERR', '/usr/bin/arch', '-k');
    close $in_38 or croak 'Close failed: $OS_ERROR';
    my $result_38 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_38> };
    close $out_38 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_38, 0;
    $result_38
} =~ /^S4.*$/msx) {
                $UNAME_RELEASE = do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; push @__parts, $__ver; join(" ", @__parts) . "\n"; };
    }
        $SUN_REL = do { my $result_39 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e s/-/_/' }; chomp $result_39; $result_39; };
        $GUESS = 'sparc-sun-sunos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun3.*:SunOS:.*:.*$/msx) {
        $GUESS = 'm68k-sun-sunos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sun.*:.*:4.2BSD:.*$/msx) {
        $UNAME_RELEASE = do { my @_qx_cmd = ("(sed 1q /etc/motd | awk '{print substr($5,1,3)}') 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        if (do {
$main_exit_code = system('test', "x$UNAME_RELEASE", q{=}, q{x}) >> 8;
        $CHILD_ERROR == 0
    }) {
                $UNAME_RELEASE = q{3};
    }
    if (do {
    my ($in_40, $out_40);
    my $pid_40 = open3($in_40, $out_40, '>&STDERR', '/bin/arch');
    close $in_40 or croak 'Close failed: $OS_ERROR';
    my $result_40 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_40> };
    close $out_40 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_40, 0;
    $result_40
} eq 'sun3') {
                $GUESS = 'm68k-sun-sunos';
                $CHILD_ERROR = 0;
    } elsif (do {
    my ($in_41, $out_41);
    my $pid_41 = open3($in_41, $out_41, '>&STDERR', '/bin/arch');
    close $in_41 or croak 'Close failed: $OS_ERROR';
    my $result_41 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_41> };
    close $out_41 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_41, 0;
    $result_41
} eq 'sun4') {
                $GUESS = 'sparc-sun-sunos';
                $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^aushp:SunOS:.*:.*$/msx) {
        $GUESS = 'sparc-auspex-sunos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atarist\[e\]:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atarist\[e\]:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atarist\[e\]:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-atari-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atari.*:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atari.*:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^atarist\[e\]:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-atari-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*falcon.*:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*falcon.*:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*falcon.*:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-atari-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^milan.*:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^milan.*:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*milan.*:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-milan-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hades.*:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hades.*:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*hades.*:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-hades-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:.*MiNT:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:.*mint:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:.*TOS:.*:.*$/msx) {
        $GUESS = 'm68k-unknown-mint';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m68k:machten:.*:.*$/msx) {
        $GUESS = 'm68k-apple-machten';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^powerpc:machten:.*:.*$/msx) {
        $GUESS = 'powerpc-apple-machten';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^RISC.*:Mach:.*:.*$/msx) {
        $GUESS = 'mips-dec-mach_bsd4.3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^RISC.*:ULTRIX:.*:.*$/msx) {
        $GUESS = 'mips-dec-ultrix';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^VAX.*:ULTRIX.*:.*:.*$/msx) {
        $GUESS = 'vax-dec-ultrix';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^2020:CLIX:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^2430:CLIX:.*:.*$/msx) {
        $GUESS = 'clipper-intergraph-clix';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips:.*:.*:UMIPS$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips:.*:.*:RISCos$/msx) {
        set_cc_for_build();
    my $temp_content = '#ifdef __cplusplus
#include <stdio.h>  /* for printf() prototype */
	int main (int argc, char *argv[]) {
#else
	int main (argc, argv) int argc; char *argv[]; {
#endif
	#if defined (host_mips) && defined (MIPSEB)
	#if defined (SYSTYPE_SYSV)
	  printf ("mips-mips-riscos%ssysv\\n", argv[1]); exit (0);
	#endif
	#if defined (SYSTYPE_SVR4)
	  printf ("mips-mips-riscos%ssvr4\\n", argv[1]); exit (0);
	#endif
	#if defined (SYSTYPE_BSD43) || defined(SYSTYPE_BSD)
	  printf ("mips-mips-riscos%sbsd\\n", argv[1]); exit (0);
	#endif
	#endif
	  exit (-1);
	}
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_1, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_1 $temp_content;
close $fh_1 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$ENV{dummy}.c"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @sed_lines_42 = split /\n/, $;
my @sed_result_42;
foreach my $line (@sed_lines_42) {
chomp $line;
$line =~ s/^	//gmsx;
push @sed_result_42, $line;
}
$ = join "\n", @sed_result_42;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
        if (do {
if (do {
if (do {
$CHILD_ERROR = 0;
    $CHILD_ERROR == 0
}) {
        $dummyarg = do { my $result_43 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -n "s/\\\\([0-9]*\\\\).*/\\\\1/p"' }; chomp $result_43; $result_43; };
}
    $CHILD_ERROR == 0
}) {
        $SYSTEM_NAME = do {
    my ($in_44, $out_44);
    my $pid_44 = open3($in_44, $out_44, '>&STDERR', "$ENV{dummy}", "$dummyarg");
    close $in_44 or croak 'Close failed: $OS_ERROR';
    my $result_44 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_44> };
    close $out_44 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_44, 0;
    $result_44
};
}
        $CHILD_ERROR == 0
    }) {
                    say $SYSTEM_NAME;
exit $main_exit_code;
    }
        $GUESS = 'mips-mips-riscos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Motorola:PowerMAX_OS:.*:.*$/msx) {
        $GUESS = 'powerpc-motorola-powermax';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Motorola:.*:4.3:PL8-.*$/msx) {
        $GUESS = 'powerpc-harris-powermax';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Night_Hawk:.*:.*:PowerMAX_OS$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Synergy:PowerMAX_OS:.*:.*$/msx) {
        $GUESS = 'powerpc-harris-powermax';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Night_Hawk:Power_UNIX:.*:.*$/msx) {
        $GUESS = 'powerpc-harris-powerunix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m88k:CX/UX:7.*:.*$/msx) {
        $GUESS = 'm88k-harris-cxux7';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m88k:.*:4.*:R4.*$/msx) {
        $GUESS = 'm88k-motorola-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m88k:.*:3.*:R3.*$/msx) {
        $GUESS = 'm88k-motorola-sysv3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^AViiON:dgux:.*:.*$/msx) {
        $UNAME_PROCESSOR = do {
    my ($in_45, $out_45);
    my $pid_45 = open3($in_45, $out_45, '>&STDERR', '/usr/bin/uname', '-p');
    close $in_45 or croak 'Close failed: $OS_ERROR';
    my $result_45 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_45> };
    close $out_45 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_45, 0;
    $result_45
};
    if ((!(system('test', "$UNAME_PROCESSOR", q{=}, 'mc88100') >> 8) || !(system('test', "$UNAME_PROCESSOR", q{=}, 'mc88110') >> 8))) {
if ((!(system('test', "$ENV{TARGET_BINARY_INTERFACE}", q{x}, q{=}, 'm88kdguxelfx') >> 8) || !(system('test', "$ENV{TARGET_BINARY_INTERFACE}", q{x}, q{=}, q{x}) >> 8))) {
            $GUESS = 'm88k-dg-dgux';
            $CHILD_ERROR = 0;
}
        else {
            $GUESS = 'm88k-dg-dguxbcs';
            $CHILD_ERROR = 0;
        }
}
    else {
        $GUESS = 'i586-dg-dgux';
        $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^M88.*:DolphinOS:.*:.*$/msx) {
        $GUESS = 'm88k-dolphin-sysv3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^M88.*:.*:R3.*:.*$/msx) {
        $GUESS = 'm88k-motorola-sysv3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^XD88.*:.*:.*:.*$/msx) {
        $GUESS = 'm88k-tektronix-sysv3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Tek43\[0-9\]\[0-9\]:UTek:.*:.*$/msx) {
        $GUESS = 'm68k-tektronix-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:IRIX.*:.*:.*$/msx) {
        $IRIX_REL = do { my $result_46 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e s/-/_/g' }; chomp $result_46; $result_46; };
        $GUESS = 'mips-sgi-irix';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^........:AIX.:\[12\].1:2$/msx) {
        $GUESS = 'romp-ibm-aix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:AIX:.*:.*$/msx) {
        $GUESS = 'i386-ibm-aix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ia64:AIX:.*:.*$/msx) {
    if ((-x '/usr/bin/oslevel')) {
        $IBM_REV = do {
    my ($in_47, $out_47);
    my $pid_47 = open3($in_47, $out_47, '>&STDERR', '/usr/bin/oslevel');
    close $in_47 or croak 'Close failed: $OS_ERROR';
    my $result_47 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_47> };
    close $out_47 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_47, 0;
    $result_47
};
}
    else {
        $IBM_REV = "$UNAME_VERSION.";
        $CHILD_ERROR = 0;
    }
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-ibm-aix', $IBM_REV) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:AIX:2:3$/msx) {
    if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $grep_result_48;
my @grep_lines_48 = ();
my @grep_filenames_48 = ();
if (-e "/usr/include/stdio.h") {
    open my $fh, '<', "/usr/include/stdio.h" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_48, $line;
        push @grep_filenames_48, "/usr/include/stdio.h";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /usr/include/stdio.h: No such file or directory\n"; }
my @grep_filtered_48 = grep { /bos325/msx } @grep_lines_48;
$grep_result_48 = join "\n", @grep_filtered_48;
        if (!($grep_result_48 =~ m{\n\z} || $grep_result_48 eq q{})) {
            $grep_result_48 .= "\n";
        }
print $grep_result_48;
$CHILD_ERROR = scalar @grep_filtered_48 > 0 ? 0 : 1;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        set_cc_for_build();
my $temp_content = '		#include <sys/systemcfg.h>

		main()
			{
			if (!__power_pc())
				exit(1);
			puts("powerpc-ibm-aix3.2.5");
			exit(0);
			}
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_2, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_2 $temp_content;
close $fh_2 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', "$ENV{dummy}.c"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
my @sed_lines_49 = split /\n/, $;
my @sed_result_49;
foreach my $line (@sed_lines_49) {
chomp $line;
$line =~ s/^		//gmsx;
push @sed_result_49, $line;
}
$ = join "\n", @sed_result_49;

            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
if ((!(        $CHILD_ERROR = 0) && !(        $SYSTEM_NAME = do {
    my ($in_50, $out_50);
    my $pid_50 = open3($in_50, $out_50, '>&STDERR', "$ENV{dummy}");
    close $in_50 or croak 'Close failed: $OS_ERROR';
    my $result_50 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_50> };
    close $out_50 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_50, 0;
    $result_50
}))) {
            $GUESS = $SYSTEM_NAME;
}
        else {
            $GUESS = 'rs6000-ibm-aix3.2.5';
        }
}
    else {
        if (!(        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $grep_result_51;
my @grep_lines_51 = ();
my @grep_filenames_51 = ();
if (-e "/usr/include/stdio.h") {
    open my $fh, '<', "/usr/include/stdio.h" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_51, $line;
        push @grep_filenames_51, "/usr/include/stdio.h";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /usr/include/stdio.h: No such file or directory\n"; }
my @grep_filtered_51 = grep { /bos324/msx } @grep_lines_51;
$grep_result_51 = join "\n", @grep_filtered_51;
            if (!($grep_result_51 =~ m{\n\z} || $grep_result_51 eq q{})) {
                $grep_result_51 .= "\n";
            }
print $grep_result_51;
$CHILD_ERROR = scalar @grep_filtered_51 > 0 ? 0 : 1;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };)) {
            $GUESS = 'rs6000-ibm-aix3.2.4';
}
        else {
            $GUESS = 'rs6000-ibm-aix3.2';
        }
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:AIX:.*:\[4567\]$/msx) {
        $IBM_CPU_ID = do { my $result_52 = qx{bash -c q(/usr/sbin/lsdev -C -c processor -S available | sed 1q | awk '{ print $1 }') }; chomp $result_52; $result_52; };
    if (!(    # Original bash: /usr/sbin/lsattr -El "$IBM_CPU_ID" | grep ' POWER' >/dev/null 2>&1;
do {
        my $output_53 = q{};
        my $output_printed_53;
        my $pipeline_success_53 = 1;
                my ($in_54, $out_54);
        my $pid_54 = open3($in_54, $out_54, '>&STDERR', '/usr/sbin/lsattr', '-El');
        close $in_54 or croak 'Close failed: $OS_ERROR';
        $output_53 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_54> };
        close $out_54 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_54, 0;

                my $grep_result_53_1;
        my @grep_lines_53_1 = split /\n/msx, $output_53;
        my @grep_filtered_53_1 = grep { /\ POWER/msx } @grep_lines_53_1;
        $grep_result_53_1 = join "\n", @grep_filtered_53_1;
        if (!($grep_result_53_1 =~ m{\n\z} || $grep_result_53_1 eq q{})) {
        $grep_result_53_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_53_1 > 0 ? 0 : 1;
        $output_53 = $grep_result_53_1;
        if ( !$pipeline_success_53 ) { $main_exit_code = 1; }
        };)) {
        $IBM_ARCH = 'rs6000';
}
    else {
        $IBM_ARCH = 'powerpc';
    }
    if ((-x '/usr/bin/lslpp')) {
        $IBM_REV = do { my $result_55 = qx{bash -c q(/usr/bin/lslpp -L qc bos.rte.libc | awk -F: '{ print $3 }' | sed 's/[0-9]*' $ /0/) }; chomp $result_55; $result_55; };
}
    else {
        $IBM_REV = "$UNAME_VERSION.";
        $CHILD_ERROR = 0;
    }
        $GUESS = $IBM_ARCH;
        $main_exit_code = system('-ibm-aix', $IBM_REV) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:AIX:.*:.*$/msx) {
        $GUESS = 'rs6000-ibm-aix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ibmrt:4.4BSD:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^romp-ibm:4.4BSD:.*$/msx) {
        $GUESS = 'romp-ibm-bsd4.4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ibmrt:.*BSD:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^romp-ibm:BSD:.*$/msx) {
        $GUESS = 'romp-ibm-bsd';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:BOSX:.*:.*$/msx) {
        $GUESS = 'rs6000-bull-bosx';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^DPX/2.00:B.O.S.:.*:.*$/msx) {
        $GUESS = 'm68k-bull-sysv3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/\[34\]..:4.3bsd:1..*:.*$/msx) {
        $GUESS = 'm68k-hp-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hp300:4.4BSD:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/\[34\]..:4.3bsd:2..*:.*$/msx) {
        $GUESS = 'm68k-hp-bsd4.4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/\[34678\]..:HP-UX:.*:.*$/msx) {
        $HPUX_REV = do { my $result_56 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*.[0B]*//'} }; chomp $result_56; $result_56; };
    if ($UNAME_MACHINE =~ /^9000/31.$/msx) {
                $HP_ARCH = 'm68000';
    } elsif ($UNAME_MACHINE =~ /^9000/\[34\]..$/msx) {
                $HP_ARCH = 'm68k';
    } elsif ($UNAME_MACHINE =~ /^9000/\[678\]\[0-9\]\[0-9\]$/msx) {
        if ((-x '/usr/bin/getconf')) {
            $sc_cpu_version = do { my @_qx_cmd = ("/usr/bin/getconf SC_CPU_VERSION 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $sc_kernel_bits = do { my @_qx_cmd = ("/usr/bin/getconf SC_KERNEL_BITS 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ($sc_cpu_version eq '523') {
                                $HP_ARCH = 'hppa1.0';
            } elsif ($sc_cpu_version eq '528') {
                                $HP_ARCH = 'hppa1.1';
            } elsif ($sc_cpu_version eq '532') {
                if ($sc_kernel_bits eq '32') {
                                        $HP_ARCH = 'hppa2.0n';
                } elsif ($sc_kernel_bits eq '64') {
                                        $HP_ARCH = 'hppa2.0w';
                } elsif ($sc_kernel_bits eq '') {
                                        $HP_ARCH = 'hppa2.0';
                }
            }
        }
        if (StringInterpolation(StringInterpolation { parts: [Variable("HP_ARCH")] }, None) eq StringInterpolation(StringInterpolation { parts: [Literal("")] }, None)) {
            set_cc_for_build();
my $temp_content = '
		#define _HPUX_SOURCE
		#include <stdlib.h>
		#include <unistd.h>

		int main ()
		{
		#if defined(_SC_KERNEL_BITS)
		    long bits = sysconf(_SC_KERNEL_BITS);
		#endif
		    long cpu  = sysconf (_SC_CPU_VERSION);

		    switch (cpu)
			{
			case CPU_PA_RISC1_0: puts ("hppa1.0"); break;
			case CPU_PA_RISC1_1: puts ("hppa1.1"); break;
			case CPU_PA_RISC2_0:
		#if defined(_SC_KERNEL_BITS)
			    switch (bits)
				{
				case 64: puts ("hppa2.0w"); break;
				case 32: puts ("hppa2.0n"); break;
				default: puts ("hppa2.0"); break;
				} break;
		#else  /* !defined(_SC_KERNEL_BITS) */
			    puts ("hppa2.0"); break;
		#endif
			default: puts ("hppa1.0"); break;
			}
		    exit (0);
		}
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_3, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_3 $temp_content;
close $fh_3 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>', "$ENV{dummy}.c"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
my @sed_lines_57 = split /\n/, $;
my @sed_result_57;
foreach my $line (@sed_lines_57) {
chomp $line;
$line =~ s/^		//gmsx;
push @sed_result_57, $line;
}
$ = join "\n", @sed_result_57;

                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            if (do {
do {
    local %ENV = %ENV;
    my $me = $me;
    my $UNAME_VERSION = $UNAME_VERSION;
    my $GNU_REL = $GNU_REL;
    my $dummyarg = $dummyarg;
    my $UNAME_REL = $UNAME_REL;
    my $IRIX_REL = $IRIX_REL;
    my $SUN_REL = $SUN_REL;
    my $HPUX_REV = $HPUX_REV;
    my $IS_GLIBC = $IS_GLIBC;
    my $abi = $abi;
    my $UNAME_MACHINE_ARCH = $UNAME_MACHINE_ARCH;
    my $CCOPTS = $CCOPTS;
    my $endian = $endian;
    my $DRAGONFLY_REL = $DRAGONFLY_REL;
    my $UNAME_RELEASE = $UNAME_RELEASE;
    my $SYSTEM_NAME = $SYSTEM_NAME;
    my $PATH = $PATH;
    my $tmp = $tmp;
    my $FUJITSU_REL = $FUJITSU_REL;
    my $cc_set_vars = $cc_set_vars;
    my $ALPHA_CPU_TYPE = $ALPHA_CPU_TYPE;
    my $OSF_REL = $OSF_REL;
    my $UNAME_PROCESSOR = $UNAME_PROCESSOR;
    my $GNU_ARCH = $GNU_ARCH;
    my $expr = $expr;
    my $UNAME_MACHINE = $UNAME_MACHINE;
    my $UNAME_SYSTEM = $UNAME_SYSTEM;
    my $timestamp = $timestamp;
    my $machine = $machine;
    my $CC_FOR_BUILD = $CC_FOR_BUILD;
    my $GNU_SYS = $GNU_SYS;
    my $os = $os;
    my $LIBCABI = $LIBCABI;
    my $sc_cpu_version = $sc_cpu_version;
    my $arch = $arch;
    my $# = $#;
    my $sc_kernel_bits = $sc_kernel_bits;
    my $FREEBSD_REL = $FREEBSD_REL;
    my $FUJITSU_PROC = $FUJITSU_PROC;
    my $SKYOS_REL = $SKYOS_REL;
    my $IBM_REV = $IBM_REV;
    my $usage = $usage;
    my $FUJITSU_SYS = $FUJITSU_SYS;
    my $cc_set_libc = $cc_set_libc;
    my $help = $help;
    my $release = $release;
    my $HP_ARCH = $HP_ARCH;
    my $SUN_ARCH = $SUN_ARCH;
    my $IBM_ARCH = $IBM_ARCH;
    my $GUESS = $GUESS;
    my $version = $version;
    my $CRAY_REL = $CRAY_REL;
    my $OS_REL = $OS_REL;
    my $IBM_CPU_ID = $IBM_CPU_ID;
        $CCOPTS = "";
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
            $CHILD_ERROR = 0;
        };
    q{};
};
                $CHILD_ERROR == 0
            }) {
                                $HP_ARCH = do {
    my ($in_58, $out_58);
    my $pid_58 = open3($in_58, $out_58, '>&STDERR', "$ENV{dummy}");
    close $in_58 or croak 'Close failed: $OS_ERROR';
    my $result_58 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_58> };
    close $out_58 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_58, 0;
    $result_58
};
            }
            if (do {
$main_exit_code = system('test', '-z', "$HP_ARCH") >> 8;
                $CHILD_ERROR == 0
            }) {
                                $HP_ARCH = 'hppa';
            }
        }
    }
    if (StringInterpolation(StringInterpolation { parts: [Variable("HP_ARCH")] }, None) eq hppa2.0w) {
        set_cc_for_build();
if (!(        # Original bash: echo __LP64__ | (CCOPTS="" $CC_FOR_BUILD -E - 2>/dev/null) |
do {
            my $output_59 = q{};
            my $output_printed_59;
            my $pipeline_success_59 = 1;
            $output_59 .= '__LP64__' . "\n";
if ( !($output_59 =~ m{\n\z}) ) { $output_59 .= "\n"; }

                        $output_59 = q{};
            my @_pcmd_61 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
            my ($in_60, $out_60);
            my $pid_60 = open3($in_60, $out_60, '>&STDERR', @_pcmd_61);
            close $in_60 or croak 'Close failed: $OS_ERROR';
            $output_59 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_60> };
            close $out_60 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_60, 0;
            my @_pcmd_63 = ('sh', '-c', '$CC_FOR_BUILD -E - 2> /dev/null');
            my ($in_62, $out_62);
            my $pid_62 = open3($in_62, $out_62, '>&STDERR', @_pcmd_63);
            close $in_62 or croak 'Close failed: $OS_ERROR';
            $output_59 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_62> };
            close $out_62 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_62, 0;

                        my $grep_result_59_2;
            my @grep_lines_59_2 = split /\n/msx, $output_59;
            my @grep_filtered_59_2 = grep { /__LP64__/msx } @grep_lines_59_2;
            $grep_result_59_2 = join "\n", @grep_filtered_59_2;
            if (!($grep_result_59_2 =~ m{\n\z} || $grep_result_59_2 eq q{})) {
            $grep_result_59_2 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_59_2 > 0 ? 0 : 1;
            $grep_result_59_2 = q{};
            $output_59 = q{};
            if ((scalar @grep_filtered_59_2) == 0) {
                $pipeline_success_59 = 0;
            }
            if ($output_59 ne q{} && !defined $output_printed_59) {
                print $output_59;
                if (!($output_59 =~ m{\n\z})) {
                    print "\n";
                }
            }
            if ( !$pipeline_success_59 ) { $main_exit_code = 1; }
            };)) {
            $HP_ARCH = 'hppa2.0w';
}
        else {
            $HP_ARCH = 'hppa64';
        }
    }
        $GUESS = $HP_ARCH;
        $main_exit_code = system('-hp-hpux', $HPUX_REV) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ia64:HP-UX:.*:.*$/msx) {
        $HPUX_REV = do { my $result_64 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*.[0B]*//'} }; chomp $result_64; $result_64; };
        $GUESS = 'ia64-hp-hpux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3050.*:HI-UX:.*:.*$/msx) {
        set_cc_for_build();
    my $temp_content = '	#include <unistd.h>
	int
	main ()
	{
	  long cpu = sysconf (_SC_CPU_VERSION);
	  /* The order matters, because CPU_IS_HP_MC68K erroneously returns
	     true for CPU_PA_RISC1_0.  CPU_IS_PA_RISC returns correct
	     results, however.  */
	  if (CPU_IS_PA_RISC (cpu))
	    {
	      switch (cpu)
		{
		  case CPU_PA_RISC1_0: puts ("hppa1.0-hitachi-hiuxwe2"); break;
		  case CPU_PA_RISC1_1: puts ("hppa1.1-hitachi-hiuxwe2"); break;
		  case CPU_PA_RISC2_0: puts ("hppa2.0-hitachi-hiuxwe2"); break;
		  default: puts ("hppa-hitachi-hiuxwe2"); break;
		}
	    }
	  else if (CPU_IS_HP_MC68K (cpu))
	    puts ("m68k-hitachi-hiuxwe2");
	  else puts ("unknown-hitachi-hiuxwe2");
	  exit (0);
	}
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_4, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_4 $temp_content;
close $fh_4 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$ENV{dummy}.c"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @sed_lines_65 = split /\n/, $;
my @sed_result_65;
foreach my $line (@sed_lines_65) {
chomp $line;
$line =~ s/^	//gmsx;
push @sed_result_65, $line;
}
$ = join "\n", @sed_result_65;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
        if (do {
if (do {
$CHILD_ERROR = 0;
    $CHILD_ERROR == 0
}) {
        $SYSTEM_NAME = do {
    my ($in_66, $out_66);
    my $pid_66 = open3($in_66, $out_66, '>&STDERR', "$ENV{dummy}");
    close $in_66 or croak 'Close failed: $OS_ERROR';
    my $result_66 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_66> };
    close $out_66 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_66, 0;
    $result_66
};
}
        $CHILD_ERROR == 0
    }) {
                    say $SYSTEM_NAME;
exit $main_exit_code;
    }
        $GUESS = 'unknown-hitachi-hiuxwe2';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/7..:4.3bsd:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/8.\[79\]:4.3bsd:.*:.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-h', 'p-bsd') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^9000/8..:4.3bsd:.*:.*$/msx) {
        $GUESS = 'hppa1.0';
        $main_exit_code = system('-h', 'p-bsd') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*9...*:MPE/iX:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*3000.*:MPE/iX:.*:.*$/msx) {
        $GUESS = 'hppa1.0';
        $main_exit_code = system('-h', 'p-mpeix') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hp7..:OSF1:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hp8.\[79\]:OSF1:.*:.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-h', 'p-osf') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hp8..:OSF1:.*:.*$/msx) {
        $GUESS = 'hppa1.0';
        $main_exit_code = system('-h', 'p-osf') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:OSF1:.*:.*$/msx) {
    if ((-x '/usr/sbin/sysversion')) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-osf1mk') >> 8;
}
    else {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-osf1') >> 8;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^parisc.*:Lites.*:.*:.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-h', 'p-lites') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^C1.*:ConvexOS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^convex:ConvexOS:C1.*:.*$/msx) {
        $GUESS = 'c1-convex-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^C2.*:ConvexOS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^convex:ConvexOS:C2.*:.*$/msx) {
    if (!(system('getsysinfo', '-f', 'scalar_acc') >> 8)) {
        say 'c32-convex-bsd';
}
    else {
        say 'c2-convex-bsd';
    }
    exit $main_exit_code;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^C34.*:ConvexOS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^convex:ConvexOS:C34.*:.*$/msx) {
        $GUESS = 'c34-convex-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^C38.*:ConvexOS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^convex:ConvexOS:C38.*:.*$/msx) {
        $GUESS = 'c38-convex-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^C4.*:ConvexOS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^convex:ConvexOS:C4.*:.*$/msx) {
        $GUESS = 'c4-convex-bsd';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^CRAY.*Y-MP:.*:.*:.*$/msx) {
        $CRAY_REL = do { my $result_67 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e "s/\\\\.[^.]*\\$/.X/"' }; chomp $result_67; $result_67; };
        $GUESS = 'ymp-cray-unicos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^CRAY.*\[A-Z\]90:.*:.*:.*$/msx) {
        # Original bash: echo "$UNAME_MACHINE"-cray-unicos"$UNAME_RELEASE" \
do {
        my $output_68 = q{};
        my $output_printed_68;
        my $pipeline_success_68 = 1;
        $output_68 .= $UNAME_MACHINE . q{ } . '-c' . q{ } . 'ray-unicos' . q{ } . $UNAME_RELEASE . "\n";
if ( !($output_68 =~ m{\n\z}) ) { $output_68 .= "\n"; }

                my @sed_lines_68 = split /\n/, $output_68;
        my @sed_result_68;
        foreach my $line (@sed_lines_68) {
        chomp $line;
        push @sed_result_68, $line;
        }
        $output_68 = join "\n", @sed_result_68;
        if ($output_68 ne q{} && !defined $output_printed_68) {
            print $output_68;
            if (!($output_68 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_68 ) { $main_exit_code = 1; }
        }
    exit $main_exit_code;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^CRAY.*TS:.*:.*:.*$/msx) {
        $CRAY_REL = do { my $result_69 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e "s/\\\\.[^.]*\\$/.X/"' }; chomp $result_69; $result_69; };
        $GUESS = 't90-cray-unicos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^CRAY.*T3E:.*:.*:.*$/msx) {
        $CRAY_REL = do { my $result_70 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e "s/\\\\.[^.]*\\$/.X/"' }; chomp $result_70; $result_70; };
        $GUESS = 'alphaev5-cray-unicosmk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^CRAY.*SV1:.*:.*:.*$/msx) {
        $CRAY_REL = do { my $result_71 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e "s/\\\\.[^.]*\\$/.X/"' }; chomp $result_71; $result_71; };
        $GUESS = 'sv1-cray-unicos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:UNICOS/mp:.*:.*$/msx) {
        $CRAY_REL = do { my $result_72 = qx{bash -c 'echo "$UNAME_RELEASE" | sed -e "s/\\\\.[^.]*\\$/.X/"' }; chomp $result_72; $result_72; };
        $GUESS = 'craynv-cray-unicosmp';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^F30\[01\]:UNIX_System_V:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^F700:UNIX_System_V:.*:.*$/msx) {
        $FUJITSU_PROC = do { my $result_73 = qx{bash -c 'uname -m | tr ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz' }; chomp $result_73; $result_73; };
        $FUJITSU_SYS = do { my $result_74 = qx{bash -c 'uname -p | tr ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz | sed -e "s/\\\\///"' }; chomp $result_74; $result_74; };
        $FUJITSU_REL = do { my $result_75 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/ /_/'} }; chomp $result_75; $result_75; };
        $GUESS = $FUJITSU_PROC;
        $main_exit_code = system('-f', 'ujitsu-', $FUJITSU_SYS, $FUJITSU_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^5000:UNIX_System_V:4..*:.*$/msx) {
        $FUJITSU_SYS = do { my $result_76 = qx{bash -c 'uname -p | tr ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz | sed -e "s/\\\\///"' }; chomp $result_76; $result_76; };
        $FUJITSU_REL = do { my $result_77 = qx{bash -c q{echo "$UNAME_RELEASE" | tr ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz | sed -e 's/ /_/'} }; chomp $result_77; $result_77; };
        $GUESS = 'sparc-fujitsu-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:BSD/386:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:BSD/OS:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Ascend\ Embedded/OS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-bsdi', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sparc.*:BSD/OS:.*:.*$/msx) {
        $GUESS = 'sparc-unknown-bsdi';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:BSD/OS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-bsdi', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm:FreeBSD:.*:.*$/msx) {
        $UNAME_PROCESSOR = do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; join(" ", @__parts) . "\n"; };
        set_cc_for_build();
    if (!(    # Original bash: echo __ARM_PCS_VFP | $CC_FOR_BUILD -E - 2>/dev/null \
do {
        my $output_78 = q{};
        my $output_printed_78;
        my $pipeline_success_78 = 1;
        $output_78 .= '__ARM_PCS_VFP' . "\n";
if ( !($output_78 =~ m{\n\z}) ) { $output_78 .= "\n"; }

                my $cmd_80 = 'unknown_command';
        my ($in_79, $out_79);
        my $pid_79 = open3($in_79, $out_79, '>&STDERR', $cmd_80, '-E', q{-});
        print {$in_79} $output_78;
        close $in_79 or croak 'Close failed: $OS_ERROR';
        $output_78 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_79> };
        close $out_79 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_79, 0;

                my $grep_result_78_2;
        my @grep_lines_78_2 = split /\n/msx, $output_78;
        my @grep_filtered_78_2 = grep { /__ARM_PCS_VFP/msx } @grep_lines_78_2;
        $grep_result_78_2 = join "\n", @grep_filtered_78_2;
        if (!($grep_result_78_2 =~ m{\n\z} || $grep_result_78_2 eq q{})) {
        $grep_result_78_2 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_78_2 > 0 ? 0 : 1;
        $grep_result_78_2 = q{};
        $output_78 = q{};
        if ((scalar @grep_filtered_78_2) == 0) {
            $pipeline_success_78 = 0;
        }
        if ($output_78 ne q{} && !defined $output_printed_78) {
            print $output_78;
            if (!($output_78 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_78 ) { $main_exit_code = 1; }
        };)) {
        $FREEBSD_REL = do { my $result_81 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-(].*//'} }; chomp $result_81; $result_81; };
        $GUESS = $UNAME_PROCESSOR;
        $main_exit_code = system('-unknown-freebsd', $FREEBSD_REL, '-gnueabi') >> 8;
}
    else {
        $FREEBSD_REL = do { my $result_82 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-(].*//'} }; chomp $result_82; $result_82; };
        $GUESS = $UNAME_PROCESSOR;
        $main_exit_code = system('-unknown-freebsd', $FREEBSD_REL, '-gnueabihf') >> 8;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:FreeBSD:.*:.*$/msx) {
        $UNAME_PROCESSOR = do {
    my ($in_83, $out_83);
    my $pid_83 = open3($in_83, $out_83, '>&STDERR', '/usr/bin/uname', '-p');
    close $in_83 or croak 'Close failed: $OS_ERROR';
    my $result_83 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_83> };
    close $out_83 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_83, 0;
    $result_83
};
    if ($UNAME_PROCESSOR eq 'amd64') {
                $UNAME_PROCESSOR = 'x86_64';
    } elsif ($UNAME_PROCESSOR eq 'i386') {
                $UNAME_PROCESSOR = 'i586';
    }
        $FREEBSD_REL = do { my $result_84 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-(].*//'} }; chomp $result_84; $result_84; };
        $GUESS = $UNAME_PROCESSOR;
        $main_exit_code = system('-unknown-freebsd', $FREEBSD_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*:CYGWIN.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-cygwin') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:MINGW64.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-mingw64') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:MINGW.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-mingw32') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:MSYS.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-msys') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*:PW.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-pw32') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:SerenityOS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-serenity') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Interix.*:.*$/msx) {
    if ($UNAME_MACHINE eq 'x86') {
                $GUESS = 'i586-pc-interix';
                $CHILD_ERROR = 0;
    } elsif ($UNAME_MACHINE eq 'authenticamd' or $UNAME_MACHINE eq 'genuineintel' or $UNAME_MACHINE eq 'EM64T') {
                $GUESS = 'x86_64-unknown-interix';
                $CHILD_ERROR = 0;
    } elsif ($UNAME_MACHINE eq 'IA64') {
                $GUESS = 'ia64-unknown-interix';
                $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*:UWIN.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-uwin') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^amd64:CYGWIN.*:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^x86_64:CYGWIN.*:.*:.*$/msx) {
        $GUESS = 'x86_64-pc-cygwin';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^prep.*:SunOS:5..*:.*$/msx) {
        $SUN_REL = do { my $result_85 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[^.]*//'} }; chomp $result_85; $result_85; };
        $GUESS = 'powerpcle-unknown-solaris2';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:GNU:.*:.*$/msx) {
        $GNU_ARCH = do { my $result_86 = qx{bash -c q{echo "$UNAME_MACHINE" | sed -e 's,[-/].*$,,'} }; chomp $result_86; $result_86; };
        $GNU_REL = do { my $result_87 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's,/.*$,,'} }; chomp $result_87; $result_87; };
        $GUESS = $GNU_ARCH;
        $main_exit_code = system('-unknown-', $LIBC, $GNU_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:GNU/.*:.*:.*$/msx) {
        $GNU_SYS = do { my $result_88 = qx{bash -c q{echo "$UNAME_SYSTEM" | sed 's,^[^/]*/,,' | tr '[:upper:]' '[:lower:]'} }; chomp $result_88; $result_88; };
        $GNU_REL = do { my $result_89 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-(].*//'} }; chomp $result_89; $result_89; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-', $GNU_SYS, $GNU_REL, q{-}, $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Minix:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-minix') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^aarch64:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^aarch64_be:Linux:.*:.*$/msx) {
        $UNAME_MACHINE = 'aarch64_be';
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^alpha:Linux:.*:.*$/msx) {
    if (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'EV5') {
                $UNAME_MACHINE = 'alphaev5';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'EV56') {
                $UNAME_MACHINE = 'alphaev56';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'PCA56') {
                $UNAME_MACHINE = 'alphapca56';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'PCA57') {
                $UNAME_MACHINE = 'alphapca56';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'EV6') {
                $UNAME_MACHINE = 'alphaev6';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } eq 'EV67') {
                $UNAME_MACHINE = 'alphaev67';
    } elsif (do { my @_qx_cmd = ("sed -n \"/^cpu model/s/^.*: \\\\(.*\\\\)/\\\\1/p\" /proc/cpuinfo 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; } =~ /^EV68.*$/msx) {
                $UNAME_MACHINE = 'alphaev68';
    }
        # Original bash: objdump --private-headers /bin/sh | grep -q ld.so.1
do {
        my $output_90 = q{};
        my $output_printed_90;
        my $pipeline_success_90 = 1;
                my ($in_91, $out_91);
        my $pid_91 = open3($in_91, $out_91, '>&STDERR', 'objdump', '--private-headers', '/bin/sh');
        close $in_91 or croak 'Close failed: $OS_ERROR';
        $output_90 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_91> };
        close $out_91 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_91, 0;

                my $grep_result_90_1;
        my @grep_lines_90_1 = split /\n/msx, $output_90;
        my @grep_filtered_90_1 = grep { /ld.so.1/msx } @grep_lines_90_1;
        $grep_result_90_1 = join "\n", @grep_filtered_90_1;
        if (!($grep_result_90_1 =~ m{\n\z} || $grep_result_90_1 eq q{})) {
        $grep_result_90_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_90_1 > 0 ? 0 : 1;
        $grep_result_90_1 = q{};
        $output_90 = q{};
        if ((scalar @grep_filtered_90_1) == 0) {
            $pipeline_success_90 = 0;
        }
        if ($output_90 ne q{} && !defined $output_printed_90) {
            print $output_90;
            if (!($output_90 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_90 ) { $main_exit_code = 1; }
        }
    if (StringInterpolation(StringInterpolation { parts: [Variable("?")] }, None) eq 0) {
        $LIBC = 'gnulibc1';
    }
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arc:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arceb:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arc32:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arc64:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm.*:Linux:.*:.*$/msx) {
        set_cc_for_build();
    if (!(    # Original bash: echo __ARM_EABI__ | $CC_FOR_BUILD -E - 2>/dev/null \
do {
        my $output_92 = q{};
        my $output_printed_92;
        my $pipeline_success_92 = 1;
        $output_92 .= '__ARM_EABI__' . "\n";
if ( !($output_92 =~ m{\n\z}) ) { $output_92 .= "\n"; }

                my $cmd_94 = 'unknown_command';
        my ($in_93, $out_93);
        my $pid_93 = open3($in_93, $out_93, '>&STDERR', $cmd_94, '-E', q{-});
        print {$in_93} $output_92;
        close $in_93 or croak 'Close failed: $OS_ERROR';
        $output_92 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_93> };
        close $out_93 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_93, 0;

                my $grep_result_92_2;
        my @grep_lines_92_2 = split /\n/msx, $output_92;
        my @grep_filtered_92_2 = grep { /__ARM_EABI__/msx } @grep_lines_92_2;
        $grep_result_92_2 = join "\n", @grep_filtered_92_2;
        if (!($grep_result_92_2 =~ m{\n\z} || $grep_result_92_2 eq q{})) {
        $grep_result_92_2 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_92_2 > 0 ? 0 : 1;
        $grep_result_92_2 = q{};
        $output_92 = q{};
        if ((scalar @grep_filtered_92_2) == 0) {
            $pipeline_success_92 = 0;
        }
        if ($output_92 ne q{} && !defined $output_printed_92) {
            print $output_92;
            if (!($output_92 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_92 ) { $main_exit_code = 1; }
        };)) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
}
    else {
if (!(        # Original bash: echo __ARM_PCS_VFP | $CC_FOR_BUILD -E - 2>/dev/null \
do {
            my $output_95 = q{};
            my $output_printed_95;
            my $pipeline_success_95 = 1;
            $output_95 .= '__ARM_PCS_VFP' . "\n";
if ( !($output_95 =~ m{\n\z}) ) { $output_95 .= "\n"; }

                        my $cmd_97 = 'unknown_command';
            my ($in_96, $out_96);
            my $pid_96 = open3($in_96, $out_96, '>&STDERR', $cmd_97, '-E', q{-});
            print {$in_96} $output_95;
            close $in_96 or croak 'Close failed: $OS_ERROR';
            $output_95 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_96> };
            close $out_96 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_96, 0;

                        my $grep_result_95_2;
            my @grep_lines_95_2 = split /\n/msx, $output_95;
            my @grep_filtered_95_2 = grep { /__ARM_PCS_VFP/msx } @grep_lines_95_2;
            $grep_result_95_2 = join "\n", @grep_filtered_95_2;
            if (!($grep_result_95_2 =~ m{\n\z} || $grep_result_95_2 eq q{})) {
            $grep_result_95_2 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_95_2 > 0 ? 0 : 1;
            $grep_result_95_2 = q{};
            $output_95 = q{};
            if ((scalar @grep_filtered_95_2) == 0) {
                $pipeline_success_95 = 0;
            }
            if ($output_95 ne q{} && !defined $output_printed_95) {
                print $output_95;
                if (!($output_95 =~ m{\n\z})) {
                    print "\n";
                }
            }
            if ( !$pipeline_success_95 ) { $main_exit_code = 1; }
            };)) {
            $GUESS = $UNAME_MACHINE;
            $main_exit_code = system('-unknown-linux-', $LIBC, 'eabi') >> 8;
}
        else {
            $GUESS = $UNAME_MACHINE;
            $main_exit_code = system('-unknown-linux-', $LIBC, 'eabihf') >> 8;
        }
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^avr32.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^cris:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-axis-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^crisv32:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-axis-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^e2k:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^frv:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hexagon:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ia64:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^k1om:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^loongarch32:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^loongarch64:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^loongarchx32:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m32r.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m68.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips64:Linux:.*:.*$/msx) {
        set_cc_for_build();
        $IS_GLIBC = q{0};
        if (do {
$main_exit_code = system('test', q{x}, ${LIBC}, q{=}, 'xgnu') >> 8;
        $CHILD_ERROR == 0
    }) {
                $IS_GLIBC = q{1};
    }
    my $temp_content = '	#undef CPU
	#undef mips
	#undef mipsel
	#undef mips64
	#undef mips64el
	#if ${IS_GLIBC} && defined(_ABI64)
	LIBCABI=gnuabi64
	#else
	#if ${IS_GLIBC} && defined(_ABIN32)
	LIBCABI=gnuabin32
	#else
	LIBCABI=${LIBC}
	#endif
	#endif

	#if ${IS_GLIBC} && defined(__mips64) && defined(__mips_isa_rev) && __mips_isa_rev>=6
	CPU=mipsisa64r6
	#else
	#if ${IS_GLIBC} && !defined(__mips64) && defined(__mips_isa_rev) && __mips_isa_rev>=6
	CPU=mipsisa32r6
	#else
	#if defined(__mips64)
	CPU=mips64
	#else
	CPU=mips
	#endif
	#endif
	#endif

	#if defined(__MIPSEL__) || defined(__MIPSEL) || defined(_MIPSEL) || defined(MIPSEL)
	MIPS_ENDIAN=el
	#else
	#if defined(__MIPSEB__) || defined(__MIPSEB) || defined(_MIPSEB) || defined(MIPSEB)
	MIPS_ENDIAN=
	#else
	MIPS_ENDIAN=
	#endif
	#endif
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_5, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_5 $temp_content;
close $fh_5 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$ENV{dummy}.c"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @sed_lines_98 = split /\n/, $;
my @sed_result_98;
foreach my $line (@sed_lines_98) {
chomp $line;
$line =~ s/^	//gmsx;
push @sed_result_98, $line;
}
$ = join "\n", @sed_result_98;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
        $cc_set_vars = do { my $result_99 = qx{bash -c '$CC_FOR_BUILD -E "$dummy.c" 2> /dev/null | grep "^CPU\\\\|^MIPS_ENDIAN\\\\|^LIBCABI"' }; chomp $result_99; $result_99; };
    do { my $eval_input = $cc_set_vars; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
        if (do {
$main_exit_code = system('test', "x$ENV{CPU}", q{!}, q{=}, q{x}) >> 8;
        $CHILD_ERROR == 0
    }) {
                    say "$ENV{CPU}" . ($ENV{MIPS_ENDIAN} // q{}) . "-unknown-linux-$LIBCABI";
exit $main_exit_code;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mips64el:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^openrisc.*:Linux:.*:.*$/msx) {
        $GUESS = 'or1k-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^or32:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^or1k.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^padre:Linux:.*:.*$/msx) {
        $GUESS = 'sparc-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^parisc64:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hppa64:Linux:.*:.*$/msx) {
        $GUESS = 'hppa64-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^parisc:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^hppa:Linux:.*:.*$/msx) {
    if (do { my $result_100 = qx{bash -c q{grep '^cpu[^a-z]*:' /proc/cpuinfo 2> /dev/null | cut -d ' ' -f 2} }; chomp $result_100; $result_100; } =~ /^PA7.*$/msx) {
                $GUESS = 'hppa1.1';
                $main_exit_code = system('-u', 'nknown-linux-', $LIBC) >> 8;
    } elsif (do { my $result_101 = qx{bash -c q{grep '^cpu[^a-z]*:' /proc/cpuinfo 2> /dev/null | cut -d ' ' -f 2} }; chomp $result_101; $result_101; } =~ /^PA8.*$/msx) {
                $GUESS = 'hppa2.0';
                $main_exit_code = system('-u', 'nknown-linux-', $LIBC) >> 8;
    } elsif (1) {
                $GUESS = 'hppa-unknown-linux-';
                $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ppc64:Linux:.*:.*$/msx) {
        $GUESS = 'powerpc64-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ppc:Linux:.*:.*$/msx) {
        $GUESS = 'powerpc-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ppc64le:Linux:.*:.*$/msx) {
        $GUESS = 'powerpc64le-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^ppcle:Linux:.*:.*$/msx) {
        $GUESS = 'powerpcle-unknown-linux-';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^riscv32:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^riscv32be:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^riscv64:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^riscv64be:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^s390:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^s390x:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-ibm-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sh64.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sh.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sparc:Linux:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^sparc64:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^tile.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^vax:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-dec-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^x86_64:Linux:.*:.*$/msx) {
        set_cc_for_build();
        $LIBCABI = $LIBC;
    if ((!StringInterpolation(StringInterpolation { parts: [Variable("CC_FOR_BUILD")] }, None) eq no_compiler_found)) {
if (!(        # Original bash: #! /bin/sh
do {
            my $output_102 = q{};
            my $output_printed_102;
            my $pipeline_success_102 = 1;
                        $output_102 = q{};
            $output_102 .= '#ifdef __ILP32__' . "\n";
            if ( !($output_102 =~ m{\n\z}) ) { $output_102 .= "\n"; }
            $output_102 .= 'IS_X32' . "\n";
            if ( !($output_102 =~ m{\n\z}) ) { $output_102 .= "\n"; }
            $output_102 .= '#endif' . "\n";
            if ( !($output_102 =~ m{\n\z}) ) { $output_102 .= "\n"; }

                        $output_102 = q{};
            my @_pcmd_104 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
            my ($in_103, $out_103);
            my $pid_103 = open3($in_103, $out_103, '>&STDERR', @_pcmd_104);
            close $in_103 or croak 'Close failed: $OS_ERROR';
            $output_102 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_103> };
            close $out_103 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_103, 0;
            my @_pcmd_106 = ('sh', '-c', '$CC_FOR_BUILD -E - 2> /dev/null');
            my ($in_105, $out_105);
            my $pid_105 = open3($in_105, $out_105, '>&STDERR', @_pcmd_106);
            close $in_105 or croak 'Close failed: $OS_ERROR';
            $output_102 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_105> };
            close $out_105 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_105, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            my $tmp_redirect_107 = q{};
            my $grep_result_108;
            my @grep_lines_108 = split /\n/msx, $output_102;
            my @grep_filtered_108 = grep { /IS_X32/msx } @grep_lines_108;
            $grep_result_108 = join "\n", @grep_filtered_108;
            if (!($grep_result_108 =~ m{\n\z} || $grep_result_108 eq q{})) {
            $grep_result_108 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_108 > 0 ? 0 : 1;
            $tmp_redirect_107 = $grep_result_108;
            $tmp_redirect_107;
            };
            print $tmp;
            if ($tmp eq q{}) { print $output_102; }
            $output_printed_102 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_102 ) { $main_exit_code = 1; }
            };)) {
            $LIBCABI = $LIBC;
            $main_exit_code = system('bash', 'x32') >> 8;
        }
    }
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-linux-', $LIBCABI) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^xtensa.*:Linux:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-linux-', $LIBC) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:DYNIX/ptx:4.*:.*$/msx) {
        $GUESS = 'i386-sequent-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:UNIX_SV:4.2MP:2..*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-sysv4.2uw', $UNAME_VERSION) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:OS/2:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-os2-emx') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:XTS-300:.*:STOP$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-stop') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:atheos:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-atheos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:syllable:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-syllable') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:LynxOS:2..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:LynxOS:3.\[01\].*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:LynxOS:4.\[02\].*:.*$/msx) {
        $GUESS = 'i386-unknown-lynxos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:.*DOS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-msdosdjgpp') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:.*:4..*:.*$/msx) {
        $UNAME_REL = do { my $result_109 = qx{bash -c 'echo "$UNAME_RELEASE" | sed "s/\\\\/MP\\$//"' }; chomp $result_109; $result_109; };
    if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $grep_result_110;
my @grep_lines_110 = ();
my @grep_filenames_110 = ();
if (-e "/usr/include/link.h") {
    open my $fh, '<', "/usr/include/link.h" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_110, $line;
        push @grep_filenames_110, "/usr/include/link.h";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /usr/include/link.h: No such file or directory\n"; }
my @grep_filtered_110 = grep { /Novell/msx } @grep_lines_110;
$grep_result_110 = join "\n", @grep_filtered_110;
        if (!($grep_result_110 =~ m{\n\z} || $grep_result_110 eq q{})) {
            $grep_result_110 .= "\n";
        }
print $grep_result_110;
$CHILD_ERROR = scalar @grep_filtered_110 > 0 ? 0 : 1;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-univel-sysv', $UNAME_REL) >> 8;
}
    else {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-sysv', $UNAME_REL) >> 8;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:.*:5:\[678\].*$/msx) {
    if (do { my $result_111 = qx{bash -c '/bin/uname -X | grep ^Machine' }; chomp $result_111; $result_111; } =~ /^.*486.*$/msx) {
                $UNAME_MACHINE = 'i486';
    } elsif (do { my $result_112 = qx{bash -c '/bin/uname -X | grep ^Machine' }; chomp $result_112; $result_112; } =~ /^.*Pentium$/msx) {
                $UNAME_MACHINE = 'i586';
    } elsif (do { my $result_113 = qx{bash -c '/bin/uname -X | grep ^Machine' }; chomp $result_113; $result_113; } =~ /^.*Pent.*$/msx or do { my $result_114 = qx{bash -c '/bin/uname -X | grep ^Machine' }; chomp $result_114; $result_114; } =~ /^.*Celeron$/msx) {
                $UNAME_MACHINE = 'i686';
    }
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-sysv', $UNAME_RELEASE, $UNAME_SYSTEM, $UNAME_VERSION) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:.*:3.2:.*$/msx) {
    if ((-f '/usr/options/cb.name')) {
        $UNAME_REL = do { my @_qx_cmd = ("sed -n 's/.*Version //p' < /usr/options/cb.name"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-isc', $UNAME_REL) >> 8;
}
    else {
        if (!(        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('/bin/uname', '-X') >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };)) {
            $UNAME_REL = do {
    my $command = q{(/bin/uname -X | grep Release | sed -e 's/.*= //')};
    my ($in, $out, $err);
    my $pid = open3($in, $out, $err, 'bash', '-c', $command);
    close $in or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result;
};
            if (do {
do {
    local %ENV = %ENV;
    my $me = $me;
    my $UNAME_VERSION = $UNAME_VERSION;
    my $GNU_REL = $GNU_REL;
    my $dummyarg = $dummyarg;
    my $UNAME_REL = $UNAME_REL;
    my $IRIX_REL = $IRIX_REL;
    my $SUN_REL = $SUN_REL;
    my $HPUX_REV = $HPUX_REV;
    my $IS_GLIBC = $IS_GLIBC;
    my $abi = $abi;
    my $UNAME_MACHINE_ARCH = $UNAME_MACHINE_ARCH;
    my $CCOPTS = $CCOPTS;
    my $endian = $endian;
    my $DRAGONFLY_REL = $DRAGONFLY_REL;
    my $UNAME_RELEASE = $UNAME_RELEASE;
    my $SYSTEM_NAME = $SYSTEM_NAME;
    my $PATH = $PATH;
    my $tmp = $tmp;
    my $FUJITSU_REL = $FUJITSU_REL;
    my $cc_set_vars = $cc_set_vars;
    my $ALPHA_CPU_TYPE = $ALPHA_CPU_TYPE;
    my $OSF_REL = $OSF_REL;
    my $UNAME_PROCESSOR = $UNAME_PROCESSOR;
    my $GNU_ARCH = $GNU_ARCH;
    my $expr = $expr;
    my $UNAME_MACHINE = $UNAME_MACHINE;
    my $UNAME_SYSTEM = $UNAME_SYSTEM;
    my $timestamp = $timestamp;
    my $machine = $machine;
    my $CC_FOR_BUILD = $CC_FOR_BUILD;
    my $GNU_SYS = $GNU_SYS;
    my $os = $os;
    my $LIBCABI = $LIBCABI;
    my $sc_cpu_version = $sc_cpu_version;
    my $arch = $arch;
    my $# = $#;
    my $sc_kernel_bits = $sc_kernel_bits;
    my $FREEBSD_REL = $FREEBSD_REL;
    my $FUJITSU_PROC = $FUJITSU_PROC;
    my $SKYOS_REL = $SKYOS_REL;
    my $IBM_REV = $IBM_REV;
    my $usage = $usage;
    my $FUJITSU_SYS = $FUJITSU_SYS;
    my $cc_set_libc = $cc_set_libc;
    my $help = $help;
    my $release = $release;
    my $HP_ARCH = $HP_ARCH;
    my $SUN_ARCH = $SUN_ARCH;
    my $IBM_ARCH = $IBM_ARCH;
    my $GUESS = $GUESS;
    my $version = $version;
    my $CRAY_REL = $CRAY_REL;
    my $OS_REL = $OS_REL;
    my $IBM_CPU_ID = $IBM_CPU_ID;
    # Original bash: /bin/uname -X|grep i80486 >/dev/null)
do {
        my $output_115 = q{};
        my $output_printed_115;
        my $pipeline_success_115 = 1;
                my ($in_116, $out_116);
        my $pid_116 = open3($in_116, $out_116, '>&STDERR', '/bin/uname', '-X');
        close $in_116 or croak 'Close failed: $OS_ERROR';
        $output_115 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_116> };
        close $out_116 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_116, 0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_117 = q{};
        my $grep_result_118;
        my @grep_lines_118 = split /\n/msx, $output_115;
        my @grep_filtered_118 = grep { /i80486/msx } @grep_lines_118;
        $grep_result_118 = join "\n", @grep_filtered_118;
        if (!($grep_result_118 =~ m{\n\z} || $grep_result_118 eq q{})) {
        $grep_result_118 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_118 > 0 ? 0 : 1;
        $tmp_redirect_117 = $grep_result_118;
        $tmp_redirect_117;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_115; }
        $output_printed_115 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_115 ) { $main_exit_code = 1; }
        }
    q{};
};
                $CHILD_ERROR == 0
            }) {
                                $UNAME_MACHINE = 'i486';
            }
            if (do {
do {
    local %ENV = %ENV;
    my $me = $me;
    my $UNAME_VERSION = $UNAME_VERSION;
    my $GNU_REL = $GNU_REL;
    my $dummyarg = $dummyarg;
    my $UNAME_REL = $UNAME_REL;
    my $IRIX_REL = $IRIX_REL;
    my $SUN_REL = $SUN_REL;
    my $HPUX_REV = $HPUX_REV;
    my $IS_GLIBC = $IS_GLIBC;
    my $abi = $abi;
    my $UNAME_MACHINE_ARCH = $UNAME_MACHINE_ARCH;
    my $CCOPTS = $CCOPTS;
    my $endian = $endian;
    my $DRAGONFLY_REL = $DRAGONFLY_REL;
    my $UNAME_RELEASE = $UNAME_RELEASE;
    my $SYSTEM_NAME = $SYSTEM_NAME;
    my $PATH = $PATH;
    my $tmp = $tmp;
    my $FUJITSU_REL = $FUJITSU_REL;
    my $cc_set_vars = $cc_set_vars;
    my $ALPHA_CPU_TYPE = $ALPHA_CPU_TYPE;
    my $OSF_REL = $OSF_REL;
    my $UNAME_PROCESSOR = $UNAME_PROCESSOR;
    my $GNU_ARCH = $GNU_ARCH;
    my $expr = $expr;
    my $UNAME_MACHINE = $UNAME_MACHINE;
    my $UNAME_SYSTEM = $UNAME_SYSTEM;
    my $timestamp = $timestamp;
    my $machine = $machine;
    my $CC_FOR_BUILD = $CC_FOR_BUILD;
    my $GNU_SYS = $GNU_SYS;
    my $os = $os;
    my $LIBCABI = $LIBCABI;
    my $sc_cpu_version = $sc_cpu_version;
    my $arch = $arch;
    my $# = $#;
    my $sc_kernel_bits = $sc_kernel_bits;
    my $FREEBSD_REL = $FREEBSD_REL;
    my $FUJITSU_PROC = $FUJITSU_PROC;
    my $SKYOS_REL = $SKYOS_REL;
    my $IBM_REV = $IBM_REV;
    my $usage = $usage;
    my $FUJITSU_SYS = $FUJITSU_SYS;
    my $cc_set_libc = $cc_set_libc;
    my $help = $help;
    my $release = $release;
    my $HP_ARCH = $HP_ARCH;
    my $SUN_ARCH = $SUN_ARCH;
    my $IBM_ARCH = $IBM_ARCH;
    my $GUESS = $GUESS;
    my $version = $version;
    my $CRAY_REL = $CRAY_REL;
    my $OS_REL = $OS_REL;
    my $IBM_CPU_ID = $IBM_CPU_ID;
    # Original bash: /bin/uname -X|grep '^Machine.*Pentium' >/dev/null)
do {
        my $output_119 = q{};
        my $output_printed_119;
        my $pipeline_success_119 = 1;
                my ($in_120, $out_120);
        my $pid_120 = open3($in_120, $out_120, '>&STDERR', '/bin/uname', '-X');
        close $in_120 or croak 'Close failed: $OS_ERROR';
        $output_119 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_120> };
        close $out_120 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_120, 0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_121 = q{};
        my $grep_result_122;
        my @grep_lines_122 = split /\n/msx, $output_119;
        my @grep_filtered_122 = grep { /^Machine.*Pentium/msx } @grep_lines_122;
        $grep_result_122 = join "\n", @grep_filtered_122;
        if (!($grep_result_122 =~ m{\n\z} || $grep_result_122 eq q{})) {
        $grep_result_122 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_122 > 0 ? 0 : 1;
        $tmp_redirect_121 = $grep_result_122;
        $tmp_redirect_121;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_119; }
        $output_printed_119 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_119 ) { $main_exit_code = 1; }
        }
    q{};
};
                $CHILD_ERROR == 0
            }) {
                                $UNAME_MACHINE = 'i586';
            }
            if (do {
do {
    local %ENV = %ENV;
    my $me = $me;
    my $UNAME_VERSION = $UNAME_VERSION;
    my $GNU_REL = $GNU_REL;
    my $dummyarg = $dummyarg;
    my $UNAME_REL = $UNAME_REL;
    my $IRIX_REL = $IRIX_REL;
    my $SUN_REL = $SUN_REL;
    my $HPUX_REV = $HPUX_REV;
    my $IS_GLIBC = $IS_GLIBC;
    my $abi = $abi;
    my $UNAME_MACHINE_ARCH = $UNAME_MACHINE_ARCH;
    my $CCOPTS = $CCOPTS;
    my $endian = $endian;
    my $DRAGONFLY_REL = $DRAGONFLY_REL;
    my $UNAME_RELEASE = $UNAME_RELEASE;
    my $SYSTEM_NAME = $SYSTEM_NAME;
    my $PATH = $PATH;
    my $tmp = $tmp;
    my $FUJITSU_REL = $FUJITSU_REL;
    my $cc_set_vars = $cc_set_vars;
    my $ALPHA_CPU_TYPE = $ALPHA_CPU_TYPE;
    my $OSF_REL = $OSF_REL;
    my $UNAME_PROCESSOR = $UNAME_PROCESSOR;
    my $GNU_ARCH = $GNU_ARCH;
    my $expr = $expr;
    my $UNAME_MACHINE = $UNAME_MACHINE;
    my $UNAME_SYSTEM = $UNAME_SYSTEM;
    my $timestamp = $timestamp;
    my $machine = $machine;
    my $CC_FOR_BUILD = $CC_FOR_BUILD;
    my $GNU_SYS = $GNU_SYS;
    my $os = $os;
    my $LIBCABI = $LIBCABI;
    my $sc_cpu_version = $sc_cpu_version;
    my $arch = $arch;
    my $# = $#;
    my $sc_kernel_bits = $sc_kernel_bits;
    my $FREEBSD_REL = $FREEBSD_REL;
    my $FUJITSU_PROC = $FUJITSU_PROC;
    my $SKYOS_REL = $SKYOS_REL;
    my $IBM_REV = $IBM_REV;
    my $usage = $usage;
    my $FUJITSU_SYS = $FUJITSU_SYS;
    my $cc_set_libc = $cc_set_libc;
    my $help = $help;
    my $release = $release;
    my $HP_ARCH = $HP_ARCH;
    my $SUN_ARCH = $SUN_ARCH;
    my $IBM_ARCH = $IBM_ARCH;
    my $GUESS = $GUESS;
    my $version = $version;
    my $CRAY_REL = $CRAY_REL;
    my $OS_REL = $OS_REL;
    my $IBM_CPU_ID = $IBM_CPU_ID;
    # Original bash: /bin/uname -X|grep '^Machine.*Pent *II' >/dev/null)
do {
        my $output_123 = q{};
        my $output_printed_123;
        my $pipeline_success_123 = 1;
                my ($in_124, $out_124);
        my $pid_124 = open3($in_124, $out_124, '>&STDERR', '/bin/uname', '-X');
        close $in_124 or croak 'Close failed: $OS_ERROR';
        $output_123 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_124> };
        close $out_124 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_124, 0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_125 = q{};
        my $grep_result_126;
        my @grep_lines_126 = split /\n/msx, $output_123;
        my @grep_filtered_126 = grep { /^Machine.*Pent\ *II/msx } @grep_lines_126;
        $grep_result_126 = join "\n", @grep_filtered_126;
        if (!($grep_result_126 =~ m{\n\z} || $grep_result_126 eq q{})) {
        $grep_result_126 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_126 > 0 ? 0 : 1;
        $tmp_redirect_125 = $grep_result_126;
        $tmp_redirect_125;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_123; }
        $output_printed_123 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_123 ) { $main_exit_code = 1; }
        }
    q{};
};
                $CHILD_ERROR == 0
            }) {
                                $UNAME_MACHINE = 'i686';
            }
            if (do {
do {
    local %ENV = %ENV;
    my $me = $me;
    my $UNAME_VERSION = $UNAME_VERSION;
    my $GNU_REL = $GNU_REL;
    my $dummyarg = $dummyarg;
    my $UNAME_REL = $UNAME_REL;
    my $IRIX_REL = $IRIX_REL;
    my $SUN_REL = $SUN_REL;
    my $HPUX_REV = $HPUX_REV;
    my $IS_GLIBC = $IS_GLIBC;
    my $abi = $abi;
    my $UNAME_MACHINE_ARCH = $UNAME_MACHINE_ARCH;
    my $CCOPTS = $CCOPTS;
    my $endian = $endian;
    my $DRAGONFLY_REL = $DRAGONFLY_REL;
    my $UNAME_RELEASE = $UNAME_RELEASE;
    my $SYSTEM_NAME = $SYSTEM_NAME;
    my $PATH = $PATH;
    my $tmp = $tmp;
    my $FUJITSU_REL = $FUJITSU_REL;
    my $cc_set_vars = $cc_set_vars;
    my $ALPHA_CPU_TYPE = $ALPHA_CPU_TYPE;
    my $OSF_REL = $OSF_REL;
    my $UNAME_PROCESSOR = $UNAME_PROCESSOR;
    my $GNU_ARCH = $GNU_ARCH;
    my $expr = $expr;
    my $UNAME_MACHINE = $UNAME_MACHINE;
    my $UNAME_SYSTEM = $UNAME_SYSTEM;
    my $timestamp = $timestamp;
    my $machine = $machine;
    my $CC_FOR_BUILD = $CC_FOR_BUILD;
    my $GNU_SYS = $GNU_SYS;
    my $os = $os;
    my $LIBCABI = $LIBCABI;
    my $sc_cpu_version = $sc_cpu_version;
    my $arch = $arch;
    my $# = $#;
    my $sc_kernel_bits = $sc_kernel_bits;
    my $FREEBSD_REL = $FREEBSD_REL;
    my $FUJITSU_PROC = $FUJITSU_PROC;
    my $SKYOS_REL = $SKYOS_REL;
    my $IBM_REV = $IBM_REV;
    my $usage = $usage;
    my $FUJITSU_SYS = $FUJITSU_SYS;
    my $cc_set_libc = $cc_set_libc;
    my $help = $help;
    my $release = $release;
    my $HP_ARCH = $HP_ARCH;
    my $SUN_ARCH = $SUN_ARCH;
    my $IBM_ARCH = $IBM_ARCH;
    my $GUESS = $GUESS;
    my $version = $version;
    my $CRAY_REL = $CRAY_REL;
    my $OS_REL = $OS_REL;
    my $IBM_CPU_ID = $IBM_CPU_ID;
    # Original bash: /bin/uname -X|grep '^Machine.*Pentium Pro' >/dev/null)
do {
        my $output_127 = q{};
        my $output_printed_127;
        my $pipeline_success_127 = 1;
                my ($in_128, $out_128);
        my $pid_128 = open3($in_128, $out_128, '>&STDERR', '/bin/uname', '-X');
        close $in_128 or croak 'Close failed: $OS_ERROR';
        $output_127 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_128> };
        close $out_128 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_128, 0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_129 = q{};
        my $grep_result_130;
        my @grep_lines_130 = split /\n/msx, $output_127;
        my @grep_filtered_130 = grep { /^Machine.*Pentium\ Pro/msx } @grep_lines_130;
        $grep_result_130 = join "\n", @grep_filtered_130;
        if (!($grep_result_130 =~ m{\n\z} || $grep_result_130 eq q{})) {
        $grep_result_130 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_130 > 0 ? 0 : 1;
        $tmp_redirect_129 = $grep_result_130;
        $tmp_redirect_129;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_127; }
        $output_printed_127 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_127 ) { $main_exit_code = 1; }
        }
    q{};
};
                $CHILD_ERROR == 0
            }) {
                                $UNAME_MACHINE = 'i686';
            }
            $GUESS = $UNAME_MACHINE;
            $main_exit_code = system('-pc-sco', $UNAME_REL) >> 8;
}
        else {
            $GUESS = $UNAME_MACHINE;
            $main_exit_code = system('bash', '-pc-sysv32') >> 8;
        }
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^pc:.*:.*:.*$/msx) {
        $GUESS = 'i586-pc-msdosdjgpp';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Intel:Mach:3.*:.*$/msx) {
        $GUESS = 'i386-pc-mach3';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^paragon:.*:.*:.*$/msx) {
        $GUESS = 'i860-intel-osf1';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i860:.*:4..*:.*$/msx) {
    if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $grep_result_131;
my @grep_lines_131 = ();
my @grep_filenames_131 = ();
if (-e "/usr/include/sys/uadmin.h") {
    open my $fh, '<', "/usr/include/sys/uadmin.h" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_131, $line;
        push @grep_filenames_131, "/usr/include/sys/uadmin.h";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /usr/include/sys/uadmin.h: No such file or directory\n"; }
my @grep_filtered_131 = grep { /Stardent/msx } @grep_lines_131;
$grep_result_131 = join "\n", @grep_filtered_131;
        if (!($grep_result_131 =~ m{\n\z} || $grep_result_131 eq q{})) {
            $grep_result_131 .= "\n";
        }
print $grep_result_131;
$CHILD_ERROR = scalar @grep_filtered_131 > 0 ? 0 : 1;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        $GUESS = 'i860-stardent-sysv';
        $CHILD_ERROR = 0;
}
    else {
        $GUESS = 'i860-unknown-sysv';
        $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mini.*:CTIX:SYS.*5:.*$/msx) {
        $GUESS = 'm68010-convergent-sysv';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" eq 'mc68k:UNIX:SYSTEM5:3.51m') {
        $GUESS = 'm68k-convergent-sysv';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^M680.0:D-NIX:5.3:.*$/msx) {
        $GUESS = 'm68k-diab-dnix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^M68.*:.*:R3V\[5678\].*:.*$/msx) {
        if (do {
$main_exit_code = system('test', '-r', '/sysV68') >> 8;
        $CHILD_ERROR == 0
    }) {
                    say 'm68k-motorola-sysv';
exit $main_exit_code;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[345\]..:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[34\]..A:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[34\]..,.*:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[34\]../.*:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^4400:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^4850:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SKA40:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SDS2:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SHG2:.*:4.0:3.0$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^S7501.*:.*:4.0:3.0$/msx) {
        $OS_REL = q{};
        if (do {
$main_exit_code = system('test', '-r', '/etc/.relid') >> 8;
        $CHILD_ERROR == 0
    }) {
                $OS_REL = q{.};
        $CHILD_ERROR = 0;
    }
        if (do {
do {
    my $output_132 = q{};
    my $output_printed_132;
    my $pipeline_success_132 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_133 = q{};

my $cmd_136 = '/bin/uname';
my ($in_135, $out_135);
my $pid_135 = open3($in_135, $out_135, '>&STDERR', $cmd_136, '-p');
print {$in_135} $output_132;
close $in_135 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_133 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_135> };
close $out_135 or croak 'Close failed: $OS_ERROR';
waitpid $pid_135, 0;
$tmp_redirect_133;
    };
    $output_132 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_137 = q{};
    my $grep_result_138;
    my @grep_lines_138 = split /\n/msx, $output_132;
    my @grep_filtered_138 = grep { /86/msx } @grep_lines_138;
    $grep_result_138 = join "\n", @grep_filtered_138;
    if (!($grep_result_138 =~ m{\n\z} || $grep_result_138 eq q{})) {
    $grep_result_138 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_138 > 0 ? 0 : 1;
    $tmp_redirect_137 = $grep_result_138;
    $tmp_redirect_137;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_132; }
    $output_printed_132 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_132 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i486-ncr-sysv4.3' . q{ } . $OS_REL;
exit $main_exit_code;
    }
        if (do {
do {
    my $output_139 = q{};
    my $output_printed_139;
    my $pipeline_success_139 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_140 = q{};

my $cmd_143 = '/bin/uname';
my ($in_142, $out_142);
my $pid_142 = open3($in_142, $out_142, '>&STDERR', $cmd_143, '-p');
print {$in_142} $output_139;
close $in_142 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_140 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_142> };
close $out_142 or croak 'Close failed: $OS_ERROR';
waitpid $pid_142, 0;
$tmp_redirect_140;
    };
    $output_139 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp_redirect_144 = q{};
    my $cmd_147 = '/bin/grep';
    my ($in_146, $out_146);
    my $pid_146 = open3($in_146, $out_146, '>&STDERR', $cmd_147, 'entium');
    print {$in_146} $output_139;
    close $in_146 or croak 'Close failed: $OS_ERROR';
    $tmp_redirect_144 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_146> };
    close $out_146 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_146, 0;
    $tmp_redirect_144;
    $output_printed_139 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_139 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i586-ncr-sysv4.3' . q{ } . $OS_REL;
exit $main_exit_code;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[34\]..:.*:4.0:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^3\[34\]..,.*:.*:4.0:.*$/msx) {
        if (do {
do {
    my $output_148 = q{};
    my $output_printed_148;
    my $pipeline_success_148 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_149 = q{};

my $cmd_152 = '/bin/uname';
my ($in_151, $out_151);
my $pid_151 = open3($in_151, $out_151, '>&STDERR', $cmd_152, '-p');
print {$in_151} $output_148;
close $in_151 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_149 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_151> };
close $out_151 or croak 'Close failed: $OS_ERROR';
waitpid $pid_151, 0;
$tmp_redirect_149;
    };
    $output_148 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_153 = q{};
    my $grep_result_154;
    my @grep_lines_154 = split /\n/msx, $output_148;
    my @grep_filtered_154 = grep { /86/msx } @grep_lines_154;
    $grep_result_154 = join "\n", @grep_filtered_154;
    if (!($grep_result_154 =~ m{\n\z} || $grep_result_154 eq q{})) {
    $grep_result_154 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_154 > 0 ? 0 : 1;
    $tmp_redirect_153 = $grep_result_154;
    $tmp_redirect_153;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_148; }
    $output_printed_148 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_148 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i486-ncr-sysv4';
exit $main_exit_code;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NCR.*:.*:4.2:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^MPRAS.*:.*:4.2:.*$/msx) {
        $OS_REL = '.3';
        if (do {
$main_exit_code = system('test', '-r', '/etc/.relid') >> 8;
        $CHILD_ERROR == 0
    }) {
                $OS_REL = q{.};
        $CHILD_ERROR = 0;
    }
        if (do {
do {
    my $output_155 = q{};
    my $output_printed_155;
    my $pipeline_success_155 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_156 = q{};

my $cmd_159 = '/bin/uname';
my ($in_158, $out_158);
my $pid_158 = open3($in_158, $out_158, '>&STDERR', $cmd_159, '-p');
print {$in_158} $output_155;
close $in_158 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_156 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_158> };
close $out_158 or croak 'Close failed: $OS_ERROR';
waitpid $pid_158, 0;
$tmp_redirect_156;
    };
    $output_155 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_160 = q{};
    my $grep_result_161;
    my @grep_lines_161 = split /\n/msx, $output_155;
    my @grep_filtered_161 = grep { /86/msx } @grep_lines_161;
    $grep_result_161 = join "\n", @grep_filtered_161;
    if (!($grep_result_161 =~ m{\n\z} || $grep_result_161 eq q{})) {
    $grep_result_161 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_161 > 0 ? 0 : 1;
    $tmp_redirect_160 = $grep_result_161;
    $tmp_redirect_160;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_155; }
    $output_printed_155 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_155 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i486-ncr-sysv4.3' . q{ } . $OS_REL;
exit $main_exit_code;
    }
        if (do {
do {
    my $output_162 = q{};
    my $output_printed_162;
    my $pipeline_success_162 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_163 = q{};

my $cmd_166 = '/bin/uname';
my ($in_165, $out_165);
my $pid_165 = open3($in_165, $out_165, '>&STDERR', $cmd_166, '-p');
print {$in_165} $output_162;
close $in_165 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_163 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_165> };
close $out_165 or croak 'Close failed: $OS_ERROR';
waitpid $pid_165, 0;
$tmp_redirect_163;
    };
    $output_162 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp_redirect_167 = q{};
    my $cmd_170 = '/bin/grep';
    my ($in_169, $out_169);
    my $pid_169 = open3($in_169, $out_169, '>&STDERR', $cmd_170, 'entium');
    print {$in_169} $output_162;
    close $in_169 or croak 'Close failed: $OS_ERROR';
    $tmp_redirect_167 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_169> };
    close $out_169 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_169, 0;
    $tmp_redirect_167;
    $output_printed_162 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_162 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i586-ncr-sysv4.3' . q{ } . $OS_REL;
exit $main_exit_code;
    }
        if (do {
do {
    my $output_171 = q{};
    my $output_printed_171;
    my $pipeline_success_171 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_172 = q{};

my $cmd_175 = '/bin/uname';
my ($in_174, $out_174);
my $pid_174 = open3($in_174, $out_174, '>&STDERR', $cmd_175, '-p');
print {$in_174} $output_171;
close $in_174 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_172 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_174> };
close $out_174 or croak 'Close failed: $OS_ERROR';
waitpid $pid_174, 0;
$tmp_redirect_172;
    };
    $output_171 = $output;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp_redirect_176 = q{};
    my $cmd_179 = '/bin/grep';
    my ($in_178, $out_178);
    my $pid_178 = open3($in_178, $out_178, '>&STDERR', $cmd_179, 'pteron');
    print {$in_178} $output_171;
    close $in_178 or croak 'Close failed: $OS_ERROR';
    $tmp_redirect_176 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_178> };
    close $out_178 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_178, 0;
    $tmp_redirect_176;
    $output_printed_171 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_171 ) { $main_exit_code = 1; }
    }
        $CHILD_ERROR == 0
    }) {
                    say 'i586-ncr-sysv4.3' . q{ } . $OS_REL;
exit $main_exit_code;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m68.*:LynxOS:2..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^m68.*:LynxOS:3.0.*:.*$/msx) {
        $GUESS = 'm68k-unknown-lynxos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mc68030:UNIX_System_V:4..*:.*$/msx) {
        $GUESS = 'm68k-atari-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^TSUNAMI:LynxOS:2..*:.*$/msx) {
        $GUESS = 'sparc-unknown-lynxos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^rs6000:LynxOS:2..*:.*$/msx) {
        $GUESS = 'rs6000-unknown-lynxos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^PowerPC:LynxOS:2..*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^PowerPC:LynxOS:3.\[01\].*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^PowerPC:LynxOS:4.\[02\].*:.*$/msx) {
        $GUESS = 'powerpc-unknown-lynxos';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SM\[BE\]S:UNIX_SV:.*:.*$/msx) {
        $GUESS = 'mips-dde-sysv';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^RM.*:ReliantUNIX-.*:.*:.*$/msx) {
        $GUESS = 'mips-sni-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^RM.*:SINIX-.*:.*:.*$/msx) {
        $GUESS = 'mips-sni-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:SINIX-.*:.*:.*$/msx) {
    if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('/bin/uname', '-p') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        $UNAME_MACHINE = do { my @_qx_cmd = ("uname -p 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-sni-sysv4') >> 8;
}
    else {
        $GUESS = 'ns32k-sni-sysv';
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^PENTIUM:.*:4.0.*:.*$/msx) {
        $GUESS = 'i586-unisys-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:UNIX_System_V:4.*:FTX.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-s', 'tratus-sysv4') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:.*:.*:FTX.*$/msx) {
        $GUESS = 'i860-stratus-sysv4';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:VOS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-stratus-vos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:VOS:.*:.*$/msx) {
        $GUESS = 'hppa1.1';
        $main_exit_code = system('-s', 'tratus-vos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^mc68.*:A/UX:.*:.*$/msx) {
        $GUESS = 'm68k-apple-aux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^news.*:NEWS-OS:6.*:.*$/msx) {
        $GUESS = 'mips-sony-newsos6';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^R\[34\]000:.*System_V.*:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^R4000:UNIX_SYSV:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^R.*000:UNIX_SV:.*:.*$/msx) {
    if ((-d '/usr/nec')) {
        $GUESS = 'mips-nec-sysv';
        $CHILD_ERROR = 0;
}
    else {
        $GUESS = 'mips-unknown-sysv';
        $CHILD_ERROR = 0;
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^BeBox:BeOS:.*:.*$/msx) {
        $GUESS = 'powerpc-be-beos';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^BeMac:BeOS:.*:.*$/msx) {
        $GUESS = 'powerpc-apple-beos';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^BePC:BeOS:.*:.*$/msx) {
        $GUESS = 'i586-pc-beos';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^BePC:Haiku:.*:.*$/msx) {
        $GUESS = 'i586-pc-haiku';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^x86_64:Haiku:.*:.*$/msx) {
        $GUESS = 'x86_64-unknown-haiku';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-4:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx4-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-5:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx5-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-6:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx6-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-7:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx7-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-8:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx8-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-8R:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sx8r-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SX-ACE:SUPER-UX:.*:.*$/msx) {
        $GUESS = 'sxace-nec-superux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^Power.*:Rhapsody:.*:.*$/msx) {
        $GUESS = 'powerpc-apple-rhapsody';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Rhapsody:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-apple-rhapsody', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^arm64:Darwin:.*:.*$/msx) {
        $GUESS = 'aarch64-apple-darwin';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Darwin:.*:.*$/msx) {
        $UNAME_PROCESSOR = do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; join(" ", @__parts) . "\n"; };
    if ($UNAME_PROCESSOR eq 'unknown') {
                $UNAME_PROCESSOR = 'powerpc';
    }
    if ((!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'xcode-select') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    }) && !(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
!($main_exit_code = system('xcode-select', '--print-path') >> 8;)
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    }))) {
        $CC_FOR_BUILD = 'no_compiler_found';
}
    else {
        set_cc_for_build();
    }
    if ((!StringInterpolation(StringInterpolation { parts: [Variable("CC_FOR_BUILD")] }, None) eq no_compiler_found)) {
if (!(        # Original bash: #! /bin/sh
do {
            my $output_180 = q{};
            my $output_printed_180;
            my $pipeline_success_180 = 1;
                        $output_180 = q{};
            $output_180 .= '#ifdef __LP64__' . "\n";
            if ( !($output_180 =~ m{\n\z}) ) { $output_180 .= "\n"; }
            $output_180 .= 'IS_64BIT_ARCH' . "\n";
            if ( !($output_180 =~ m{\n\z}) ) { $output_180 .= "\n"; }
            $output_180 .= '#endif' . "\n";
            if ( !($output_180 =~ m{\n\z}) ) { $output_180 .= "\n"; }

                        $output_180 = q{};
            my @_pcmd_182 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
            my ($in_181, $out_181);
            my $pid_181 = open3($in_181, $out_181, '>&STDERR', @_pcmd_182);
            close $in_181 or croak 'Close failed: $OS_ERROR';
            $output_180 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_181> };
            close $out_181 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_181, 0;
            my @_pcmd_184 = ('sh', '-c', '$CC_FOR_BUILD -E - 2> /dev/null');
            my ($in_183, $out_183);
            my $pid_183 = open3($in_183, $out_183, '>&STDERR', @_pcmd_184);
            close $in_183 or croak 'Close failed: $OS_ERROR';
            $output_180 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_183> };
            close $out_183 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_183, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            my $tmp_redirect_185 = q{};
            my $grep_result_186;
            my @grep_lines_186 = split /\n/msx, $output_180;
            my @grep_filtered_186 = grep { /IS_64BIT_ARCH/msx } @grep_lines_186;
            $grep_result_186 = join "\n", @grep_filtered_186;
            if (!($grep_result_186 =~ m{\n\z} || $grep_result_186 eq q{})) {
            $grep_result_186 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_186 > 0 ? 0 : 1;
            $tmp_redirect_185 = $grep_result_186;
            $tmp_redirect_185;
            };
            print $tmp;
            if ($tmp eq q{}) { print $output_180; }
            $output_printed_180 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_180 ) { $main_exit_code = 1; }
            };)) {
if ($UNAME_PROCESSOR eq 'i386') {
                                $UNAME_PROCESSOR = 'x86_64';
            } elsif ($UNAME_PROCESSOR eq 'powerpc') {
                                $UNAME_PROCESSOR = 'powerpc64';
            }
        }
if (!(        # Original bash: #! /bin/sh
do {
            my $output_187 = q{};
            my $output_printed_187;
            my $pipeline_success_187 = 1;
                        $output_187 = q{};
            $output_187 .= '#ifdef __POWERPC__' . "\n";
            if ( !($output_187 =~ m{\n\z}) ) { $output_187 .= "\n"; }
            $output_187 .= 'IS_PPC' . "\n";
            if ( !($output_187 =~ m{\n\z}) ) { $output_187 .= "\n"; }
            $output_187 .= '#endif' . "\n";
            if ( !($output_187 =~ m{\n\z}) ) { $output_187 .= "\n"; }

                        $output_187 = q{};
            my @_pcmd_189 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
            my ($in_188, $out_188);
            my $pid_188 = open3($in_188, $out_188, '>&STDERR', @_pcmd_189);
            close $in_188 or croak 'Close failed: $OS_ERROR';
            $output_187 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_188> };
            close $out_188 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_188, 0;
            my @_pcmd_191 = ('sh', '-c', '$CC_FOR_BUILD -E - 2> /dev/null');
            my ($in_190, $out_190);
            my $pid_190 = open3($in_190, $out_190, '>&STDERR', @_pcmd_191);
            close $in_190 or croak 'Close failed: $OS_ERROR';
            $output_187 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_190> };
            close $out_190 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_190, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            my $tmp_redirect_192 = q{};
            my $grep_result_193;
            my @grep_lines_193 = split /\n/msx, $output_187;
            my @grep_filtered_193 = grep { /IS_PPC/msx } @grep_lines_193;
            $grep_result_193 = join "\n", @grep_filtered_193;
            if (!($grep_result_193 =~ m{\n\z} || $grep_result_193 eq q{})) {
            $grep_result_193 .= "\n";
            }
            $CHILD_ERROR = scalar @grep_filtered_193 > 0 ? 0 : 1;
            $tmp_redirect_192 = $grep_result_193;
            $tmp_redirect_192;
            };
            print $tmp;
            if ($tmp eq q{}) { print $output_187; }
            $output_printed_187 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_187 ) { $main_exit_code = 1; }
            };)) {
            $UNAME_PROCESSOR = 'powerpc';
        }
}
    else {
        if (StringInterpolation(StringInterpolation { parts: [Variable("UNAME_PROCESSOR")] }, None) eq i386) {
            $UNAME_PROCESSOR = $UNAME_MACHINE;
        }
    }
        $GUESS = $UNAME_PROCESSOR;
        $main_exit_code = system('-apple-darwin', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:procnto.*:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:QNX:\[0123456789\].*:.*$/msx) {
        $UNAME_PROCESSOR = do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; join(" ", @__parts) . "\n"; };
    if (StringInterpolation(StringInterpolation { parts: [Variable("UNAME_PROCESSOR")] }, None) eq x86) {
        $UNAME_PROCESSOR = 'i386';
        $UNAME_MACHINE = 'pc';
    }
        $GUESS = $UNAME_PROCESSOR;
        $main_exit_code = system('-', $UNAME_MACHINE, '-nto-qnx', $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:QNX:.*:4.*$/msx) {
        $GUESS = 'i386-pc-qnx';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NEO-.*:NONSTOP_KERNEL:.*:.*$/msx) {
        $GUESS = 'neo-tandem-nsk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NSE-.*:NONSTOP_KERNEL:.*:.*$/msx) {
        $GUESS = 'nse-tandem-nsk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NSR-.*:NONSTOP_KERNEL:.*:.*$/msx) {
        $GUESS = 'nsr-tandem-nsk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NSV-.*:NONSTOP_KERNEL:.*:.*$/msx) {
        $GUESS = 'nsv-tandem-nsk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^NSX-.*:NONSTOP_KERNEL:.*:.*$/msx) {
        $GUESS = 'nsx-tandem-nsk';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:NonStop-UX:.*:.*$/msx) {
        $GUESS = 'mips-compaq-nonstopux';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^BS2000:POSIX.*:.*:.*$/msx) {
        $GUESS = 'bs2000-siemens-sysv';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^DS/.*:UNIX_System_V:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-', $UNAME_SYSTEM, q{-}, $UNAME_RELEASE) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Plan9:.*:.*$/msx) {
    if (StringInterpolation(StringInterpolation { parts: [ParameterExpansion(ParameterExpansion { variable: "cputype", operator: DefaultValue(""), is_mutable: true })] }, None) eq 386) {
        $UNAME_MACHINE = 'i386';
}
    else {
        if ((!StringInterpolation(StringInterpolation { parts: [Literal("x"), ParameterExpansion(ParameterExpansion { variable: "cputype", operator: DefaultValue(""), is_mutable: true })] }, None) eq x)) {
            $UNAME_MACHINE = $cputype;
        }
    }
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-plan9') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:TOPS-10:.*:.*$/msx) {
        $GUESS = 'pdp10-unknown-tops10';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:TENEX:.*:.*$/msx) {
        $GUESS = 'pdp10-unknown-tenex';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^KS10:TOPS-20:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^KL10:TOPS-20:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^TYPE4:TOPS-20:.*:.*$/msx) {
        $GUESS = 'pdp10-dec-tops20';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^XKL-1:TOPS-20:.*:.*$/msx or "$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^TYPE5:TOPS-20:.*:.*$/msx) {
        $GUESS = 'pdp10-xkl-tops20';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:TOPS-20:.*:.*$/msx) {
        $GUESS = 'pdp10-unknown-tops20';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:ITS:.*:.*$/msx) {
        $GUESS = 'pdp10-unknown-its';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^SEI:.*:.*:SEIUX$/msx) {
        $GUESS = 'mips-sei-seiux';
        $CHILD_ERROR = 0;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:DragonFly:.*:.*$/msx) {
        $DRAGONFLY_REL = do { my $result_194 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/[-(].*//'} }; chomp $result_194; $result_194; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-dragonfly', $DRAGONFLY_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:.*VMS:.*:.*$/msx) {
        $UNAME_MACHINE = do { my @_qx_cmd = ("uname -p 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    if ($UNAME_MACHINE =~ /^A.*$/msx) {
                $GUESS = 'alpha-dec-vms';
    } elsif ($UNAME_MACHINE =~ /^I.*$/msx) {
                $GUESS = 'ia64-dec-vms';
    } elsif ($UNAME_MACHINE =~ /^V.*$/msx) {
                $GUESS = 'vax-dec-vms';
    }
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:XENIX:.*:SysV$/msx) {
        $GUESS = 'i386-pc-xenix';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:skyos:.*:.*$/msx) {
        $SKYOS_REL = do { my $result_195 = qx{bash -c q{echo "$UNAME_RELEASE" | sed -e 's/ .*$//'} }; chomp $result_195; $result_195; };
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-pc-skyos', $SKYOS_REL) >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:rdos:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-rdos') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^i.*86:Fiwix:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-pc-fiwix') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:AROS:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-aros') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^x86_64:VMkernel:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('bash', '-unknown-esx') >> 8;
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^amd64:Isilon\ OneFS:.*:.*$/msx) {
        $GUESS = 'x86_64-unknown-onefs';
} elsif ("$UNAME_MACHINE:$UNAME_SYSTEM:$UNAME_RELEASE:$UNAME_VERSION" =~ /^.*:Unleashed:.*:.*$/msx) {
        $GUESS = $UNAME_MACHINE;
        $main_exit_code = system('-unknown-unleashed', $UNAME_RELEASE) >> 8;
}
if ((!StringInterpolation(StringInterpolation { parts: [Literal("x"), Variable("GUESS")] }, None) eq x)) {
    say $GUESS;
exit $main_exit_code;
}
set_cc_for_build();
open my $fh_cat, '>', "\"\$ENV{dummy}.c\"" or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "#ifdef _SEQUENT_
#include <sys/types.h>
#include <sys/utsname.h>
#endif
#if defined(ultrix) || defined(_ultrix) || defined(__ultrix) || defined(__ultrix__)
#if defined (vax) || defined (__vax) || defined (__vax__) || defined(mips) || defined(__mips) || defined(__mips__) || defined(MIPS) || defined(__MIPS__)
#include <signal.h>
#if defined(_SIZE_T_) || defined(SIGLOST)
#include <sys/utsname.h>
#endif
#endif
#endif
main ()
{
#if defined (sony)
#if defined (MIPSEB)
  /* BFD wants \"bsd\" instead of \"newsos\".  Perhaps BFD should be changed,
     I don't know....  */
  printf (\"mips-sony-bsd\\n\"); exit (0);
#else
#include <sys/param.h>
  printf (\"m68k-sony-newsos%s\\n\",
#ifdef NEWSOS4
  \"4\"
#else
  \"\"
#endif
  ); exit (0);
#endif
#endif

#if defined (NeXT)
#if !defined (__ARCHITECTURE__)
#define __ARCHITECTURE__ \"m68k\"
#endif
  int version;
  version=`(hostinfo | sed -n 's/.*NeXT Mach \\([0-9]*\\).*/\\1/p') 2>/dev/null`;
  if (version < 4)
    printf (\"%s-next-nextstep%d\\n\", __ARCHITECTURE__, version);
  else
    printf (\"%s-next-openstep%d\\n\", __ARCHITECTURE__, version);
  exit (0);
#endif

#if defined (MULTIMAX) || defined (n16)
#if defined (UMAXV)
  printf (\"ns32k-encore-sysv\\n\"); exit (0);
#else
#if defined (CMU)
  printf (\"ns32k-encore-mach\\n\"); exit (0);
#else
  printf (\"ns32k-encore-bsd\\n\"); exit (0);
#endif
#endif
#endif

#if defined (__386BSD__)
  printf (\"i386-pc-bsd\\n\"); exit (0);
#endif

#if defined (sequent)
#if defined (i386)
  printf (\"i386-sequent-dynix\\n\"); exit (0);
#endif
#if defined (ns32000)
  printf (\"ns32k-sequent-dynix\\n\"); exit (0);
#endif
#endif

#if defined (_SEQUENT_)
  struct utsname un;

  uname(&un);
  if (strncmp(un.version, \"V2\", 2) == 0) {
    printf (\"i386-sequent-ptx2\\n\"); exit (0);
  }
  if (strncmp(un.version, \"V1\", 2) == 0) { /* XXX is V1 correct? */
    printf (\"i386-sequent-ptx1\\n\"); exit (0);
  }
  printf (\"i386-sequent-ptx\\n\"); exit (0);
#endif

#if defined (vax)
#if !defined (ultrix)
#include <sys/param.h>
#if defined (BSD)
#if BSD == 43
  printf (\"vax-dec-bsd4.3\\n\"); exit (0);
#else
#if BSD == 199006
  printf (\"vax-dec-bsd4.3reno\\n\"); exit (0);
#else
  printf (\"vax-dec-bsd\\n\"); exit (0);
#endif
#endif
#else
  printf (\"vax-dec-bsd\\n\"); exit (0);
#endif
#else
#if defined(_SIZE_T_) || defined(SIGLOST)
  struct utsname un;
  uname (&un);
  printf (\"vax-dec-ultrix%s\\n\", un.release); exit (0);
#else
  printf (\"vax-dec-ultrix\\n\"); exit (0);
#endif
#endif
#endif
#if defined(ultrix) || defined(_ultrix) || defined(__ultrix) || defined(__ultrix__)
#if defined(mips) || defined(__mips) || defined(__mips__) || defined(MIPS) || defined(__MIPS__)
#if defined(_SIZE_T_) || defined(SIGLOST)
  struct utsname *un;
  uname (&un);
  printf (\"mips-dec-ultrix%s\\n\", un.release); exit (0);
#else
  printf (\"mips-dec-ultrix\\n\"); exit (0);
#endif
#endif
#endif

#if defined (alliant) && defined (i860)
  printf (\"i860-alliant-bsd\\n\"); exit (0);
#endif

  exit (1);
}
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
if (do {
if (do {
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        $CHILD_ERROR = 0;
    };
} == 0) {
        $SYSTEM_NAME = do {
    my ($in_196, $out_196);
    my $pid_196 = open3($in_196, $out_196, '>&STDERR', "$ENV{dummy}");
    close $in_196 or croak 'Close failed: $OS_ERROR';
    my $result_196 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_196> };
    close $out_196 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_196, 0;
    $result_196
};
}
    $CHILD_ERROR == 0
}) {
            say $SYSTEM_NAME;
exit $main_exit_code;
}
if (do {
$main_exit_code = system('test', '-d', '/usr/apollo') >> 8;
    $CHILD_ERROR == 0
}) {
            say "$ENV{ISP}-apollo-$ENV{SYSTYPE}";
exit $main_exit_code;
}
do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
    say "$PROGRAM_NAME: unable to guess " . "sys" . "tem" . " type";
};
if ("$UNAME_MACHINE:$UNAME_SYSTEM" eq 'mips:Linux' or "$UNAME_MACHINE:$UNAME_SYSTEM" eq 'mips64:Linux') {
    print "
NOTE: MIPS GNU/Linux systems require a C compiler to fully recognize
the system type. Please install a C compiler and try again.
";
}
print "
This script (version $timestamp), has failed to recognize the
operating system you are using. If your script is old, overwrite *all*
copies of config.guess and config.sub with the latest versions from:

  https://git.savannah.gnu.org/cgit/config.git/plain/config.guess
and
  https://git.savannah.gnu.org/cgit/config.git/plain/config.sub
";
my $our_year = do { my $result_197 = qx{bash -c q{echo Variable("timestamp", false, None) | sed 's,-.*,,'} }; chomp $result_197; $result_197; };
my $thisyear = do {
require POSIX; POSIX::strftime('%Y', localtime())
};
my $script_age = do {
    my ($in_198, $out_198);
    my $pid_198 = open3($in_198, $out_198, '>&STDERR', 'expr', "$thisyear", q{-}, "$our_year");
    close $in_198 or croak 'Close failed: $OS_ERROR';
    my $result_198 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_198> };
    close $out_198 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_198, 0;
    $result_198
};
if ((StringInterpolation(StringInterpolation { parts: [Variable("script_age")] }, None) < 3)) {
print "
If $0 has already been updated, send the following data and any
information you think might be pertinent to config-patches@gnu.org to
provide the necessary information to handle your system.

config.guess timestamp = $timestamp

uname -m = `(uname -m) 2>/dev/null || echo unknown`
uname -r = `(uname -r) 2>/dev/null || echo unknown`
uname -s = `(uname -s) 2>/dev/null || echo unknown`
uname -v = `(uname -v) 2>/dev/null || echo unknown`

/usr/bin/uname -p = `(/usr/bin/uname -p) 2>/dev/null`
/bin/uname -X     = `(/bin/uname -X) 2>/dev/null`

hostinfo               = `(hostinfo) 2>/dev/null`
/bin/universe          = `(/bin/universe) 2>/dev/null`
/usr/bin/arch -k       = `(/usr/bin/arch -k) 2>/dev/null`
/bin/arch              = `(/bin/arch) 2>/dev/null`
/usr/bin/oslevel       = `(/usr/bin/oslevel) 2>/dev/null`
/usr/convex/getsysinfo = `(/usr/convex/getsysinfo) 2>/dev/null`

UNAME_MACHINE = \"$UNAME_MACHINE\"
UNAME_RELEASE = \"$UNAME_RELEASE\"
UNAME_SYSTEM  = \"$UNAME_SYSTEM\"
UNAME_VERSION = \"$UNAME_VERSION\"
";
}
exit 1;

exit $main_exit_code;
}
