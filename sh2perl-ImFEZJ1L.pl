#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $STATE_FILE;
my $realshell;
my $line;
my $NOACT;
my $shell;

$__set_e = 1;

sub hashset_contains {
if ("$_[0]" =~ /^.*"#$2#".*$/msx) {
        return q{0};    } elsif (1) {
        return q{1};    }
    return;
}

sub log {
if ("$VERBOSE" eq 1) {
        say $*;
    }
    return;
}
my $ROOT = (defined ($ENV{DPKG_ROOT} // q{}) && ($ENV{DPKG_ROOT} // q{}) ne q{} ? ($ENV{DPKG_ROOT} // q{}) : '');
my $VERBOSE = q{0};
$NOACT = q{0};
while ( scalar(@ARGV) > 0 ) {
if ("$_[0]" eq '--help') {
        print "usage: $0 [options]

 --no-act    Do not move the actual update into place
 --verbose   Be more verbose
 --root DIR  Operate on the given chroot, defaults to /
";
        exit 0;
    } elsif ("$_[0]" eq '--no-act') {
                $NOACT = q{1};
    } elsif ("$_[0]" eq '--root') {
        # Builtin command 'shift' not implemented
        if ((scalar(@ARGV) < 1)) {
            do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
                say "missing argument to --root";
            };
exit 1;
        }
                $ROOT = $1;
    } elsif ("$_[0]" eq '--verbose') {
                $VERBOSE = q{1};
    } elsif (1) {
                do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
            say "unrecognized option $_[0]";
        };
        exit 1;
    }
# Builtin command 'shift' not implemented
}
my $PKG_DIR = "$ROOT/usr/share/debianutils/shells.d";
$STATE_FILE = "$ROOT/var/lib/shells.state";
my $TEMPLATE_ETC_FILE = "$ROOT/usr/share/debianutils/shells";
my $TARGET_ETC_FILE = "$ROOT/etc/shells";
my $SOURCE_ETC_FILE = "$TARGET_ETC_FILE";
my $NEW_ETC_FILE = "$TARGET_ETC_FILE.tmp";
my $NEW_STATE_FILE = "$STATE_FILE.tmp";
if (system('test', '-e', "$SOURCE_ETC_FILE") >> 8) {
    $SOURCE_ETC_FILE = "$TEMPLATE_ETC_FILE";
}
my $PKG_SHELLS = q{;
my $LC_COLLATE = 'C.UTF-8';
my $f;
for my $f ("$TEMPLATE_ETC_FILE", "$PKG_DIR/", q{*}) {
        $main_exit_code = system('test', '-f', "$f") >> 8;
    if ($CHILD_ERROR != 0) {
        next;
    }
;
open STDIN, '<', "$f" or croak "Cannot read file: $OS_ERROR\n";
    my $line;
    my $_;
while ( my $L = <> ) {
    chomp $L;
    my @_fields = split /\#/msx, $L;
    $line = $_fields[0] // q{};
    $_ = $_fields[1] // q{};
        if (!("$line" ne q{})) {
            next;
        }
        $PKG_SHELLS = "$PKG_SHELLS$line#";
        $realshell = do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', 'dpkg-realpath', '--root', "$ROOT", (do { use File::Basename qw(dirname); my $dirname_output = dirname("$line"); $CHILD_ERROR = 0; $dirname_output; }));
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
};
        $main_exit_code = system('/', do { use File::Basename qw(basename); my $basename_output = basename("$line"); $CHILD_ERROR = 0; $basename_output; }) >> 8;
if ("$line" ne "$realshell") {
            $PKG_SHELLS = "$PKG_SHELLS$realshell#";
        }
    }
;
}
my $STATE_SHELLS = q{;
if ((-e "$STATE_FILE")) {
open STDIN, '<', "$STATE_FILE" or croak "Cannot read file: $OS_ERROR\n";
while ( my $L = <> ) {
    chomp $L;
    my @_fields = split /\#/msx, $L;
    $line = $_fields[0] // q{};
    $_ = $_fields[1] // q{};
        if ("$line" ne q{}) {
                        $STATE_SHELLS = "$STATE_SHELLS$line#";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
    }
;
}

sub cleanup {
    unlink('$NEW_ETC_FILE');
    unlink('$NEW_STATE_FILE');
    return;
}
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx'cleanup 2>&1'; print $end_out if $end_out ne q{}; }
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', "$NEW_ETC_FILE"
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    $main_exit_code = system('bash', ':') >> 8;
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $ETC_SHELLS = q{;
open STDIN, '<', "$SOURCE_ETC_FILE" or croak "Cannot read file: $OS_ERROR\n";
while ( my $L = <> ) {
    chomp $L;
    my @_fields = split //msx, $L;
    $line = $_fields[0] // q{};
    $shell = ${line} =~ s/;
if ((("$shell" eq q{} || !(    hashset_contains("$PKG_SHELLS", "$shell"))) || !(!(hashset_contains("$STATE_SHELLS", "$shell");)))) {
if (("$shell" eq q{} || !(!(hashset_contains("$ETC_SHELLS", "$shell");)))) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$NEW_ETC_FILE"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say $line;
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            $ETC_SHELLS = "$ETC_SHELLS$shell#";
        }
}
    else {
        log("removing shell $shell");
    }
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', "$NEW_STATE_FILE"
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    $main_exit_code = system('bash', ':') >> 8;
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $saved_IFS = $IFS;
my $IFS = q{;
# set -f not implemented
# set -- not implemented
# set +f not implemented
$IFS = $saved_IFS;
for my $shell () {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$NEW_STATE_FILE"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say $shell;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
if (    if (do {
hashset_contains("$ETC_SHELLS", "$shell");
        $CHILD_ERROR == 0
    }) {
        !(hashset_contains("$STATE_SHELLS", "$shell");)
    }) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$NEW_ETC_FILE"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say $shell;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        log("adding shell $shell");
    }
}
if ("$NOACT" eq 0) {
if ((-e "$STATE_FILE")) {
        $CHILD_ERROR = 0;
        if ($CHILD_ERROR != 0) {
            chmod(oct(do {
    my ($in_3, $out_3);
    my $pid_3 = open3($in_3, $out_3, '>&STDERR', 'stat', '-c', '%a', ${STATE_FILE});
    close $in_3 or croak 'Close failed: $OS_ERROR';
    my $result_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_3> };
    close $out_3 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_3, 0;
    $result_3
}), (${NEW_STATE_FILE})) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
        }
;
        $CHILD_ERROR = 0;
        if ($CHILD_ERROR != 0) {
            do {
    my ($owner, $group) = split /:/, do {
    my ($in_6, $out_6);
    my $pid_6 = open3($in_6, $out_6, '>&STDERR', 'stat', '-c', '%U', ${STATE_FILE});
    close $in_6 or croak 'Close failed: $OS_ERROR';
    my $result_6 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_6> };
    close $out_6 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_6, 0;
    $result_6
}, 2;
    my $uid = getpwnam($owner);
    my $gid = defined($group) ? getgrnam($group) : -1;
    chown $uid, $gid, (${NEW_STATE_FILE}) or warn "chown failed: $OS_ERROR\n";
    $CHILD_ERROR = 0;
};
        }
;
}
    else {
chmod(oct(q{0644}), ("$NEW_STATE_FILE")) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
    }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) {
        chmod(oct(do {
    my ($in_10, $out_10);
    my $pid_10 = open3($in_10, $out_10, '>&STDERR', 'stat', '-c', '%a', ${SOURCE_ETC_FILE});
    close $in_10 or croak 'Close failed: $OS_ERROR';
    my $result_10 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_10> };
    close $out_10 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_10, 0;
    $result_10
}), (${NEW_ETC_FILE})) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
    }
;
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) {
        do {
    my ($owner, $group) = split /:/, do {
    my ($in_13, $out_13);
    my $pid_13 = open3($in_13, $out_13, '>&STDERR', 'stat', '-c', '%U', ${SOURCE_ETC_FILE});
    close $in_13 or croak 'Close failed: $OS_ERROR';
    my $result_13 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_13> };
    close $out_13 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_13, 0;
    $result_13
}, 2;
    my $uid = getpwnam($owner);
    my $gid = defined($group) ? getgrnam($group) : -1;
    chown $uid, $gid, (${NEW_ETC_FILE}) or warn "chown failed: $OS_ERROR\n";
    $CHILD_ERROR = 0;
};
    }
;
    $main_exit_code = system('sync', '-d', "$NEW_ETC_FILE", "$NEW_STATE_FILE") >> 8;
    do {
    my $mv_cmd_str = 'mv -Z "${NEW_ETC_FILE}" "${TARGET_ETC_FILE}"';
    system $mv_cmd_str;
};
    if ($CHILD_ERROR != 0) {
                my $err;
        my $force = 0;
        if ( -e "${NEW_ETC_FILE}" ) {
            my $dest = ${TARGET_ETC_FILE};
            if ( -e $dest && -d $dest ) {
                my $source_name = "${NEW_ETC_FILE}";
                $source_name =~ s{^.*[\/]}{};
                $dest = "$dest/$source_name";
            }
            if ( -e $dest && !$force ) {
                croak "mv: $dest: File exists (use -f to force overwrite)\n";
            }
            my $dest_dir = $dest;
            $dest_dir =~ s/\/[^\/]*$//msx;
            if ( $dest_dir eq $dest ) {
                $dest_dir = q{};
            }
            if ( $dest_dir ne q{} && !-d $dest_dir ) {
                my $err;
                make_path( $dest_dir, { error => \$err } );
                if ( @{$err} ) {
                    croak "mv: cannot create directory $dest_dir: $err->[0]\n";
                }
            }
            require File::Copy;
            if ( File::Copy::move( "${NEW_ETC_FILE}", $dest ) ) {
            } else {
                croak
  "mv: cannot move "${NEW_ETC_FILE}" to $dest: $ERRNO\n";
            }
        } else {
            croak "mv: "${NEW_ETC_FILE}": No such file or directory\n";
        }
    }
;
    $main_exit_code = system('sync', "$TARGET_ETC_FILE") >> 8;
    $main_exit_code = system('sync', (do { use File::Basename qw(dirname); my $dirname_output = dirname("$TARGET_ETC_FILE"); $CHILD_ERROR = 0; $dirname_output; })) >> 8;
    if ( -e "$NEW_STATE_FILE" ) {
        my $dest = "$STATE_FILE";
        if ( -e $dest && -d $dest ) {
            my $source_name = "$NEW_STATE_FILE";
            $source_name =~ s{^.*[\/]}{};
            $dest = "$dest/$source_name";
        }
        if ( -e $dest && !$force ) {
            croak "mv: $dest: File exists (use -f to force overwrite)\n";
        }
        my $dest_dir = $dest;
        $dest_dir =~ s/\/[^\/]*$//msx;
        if ( $dest_dir eq $dest ) {
            $dest_dir = q{};
        }
        if ( $dest_dir ne q{} && !-d $dest_dir ) {
            my $err;
            make_path( $dest_dir, { error => \$err } );
            if ( @{$err} ) {
                croak "mv: cannot create directory $dest_dir: $err->[0]\n";
            }
        }
        require File::Copy;
        if ( File::Copy::move( "$NEW_STATE_FILE", $dest ) ) {
        } else {
            croak
  "mv: cannot move "$NEW_STATE_FILE" to $dest: $ERRNO\n";
        }
    } else {
        croak "mv: "$NEW_STATE_FILE": No such file or directory\n";
    }
    $main_exit_code = system('sync', "$STATE_FILE") >> 8;
    $main_exit_code = system('sync', (do { use File::Basename qw(dirname); my $dirname_output = dirname("$STATE_FILE"); $CHILD_ERROR = 0; $dirname_output; })) >> 8;
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx' 2>&1'; print $end_out if $end_out ne q{}; }
;
}

exit $main_exit_code;
}
}
}
}
