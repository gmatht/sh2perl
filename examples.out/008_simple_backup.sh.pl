#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR = 0;
$0 = '008_simple_backup.sh';
print "Hello, World!\n";
$CHILD_ERROR = 0;
my $d = do { open(my $__fh, '-|', 'bash', '-c', 'mktemp -d') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
use File::Path qw(make_path);
my $err;
if ( !-d "${d}/sub1" ) {
    make_path( "${d}/sub1", { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . "${d}/sub1" . ": $err->[0]\n";
    }
}
if ( !-d "${d}/sub2" ) {
    make_path( "${d}/sub2", { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . "${d}/sub2" . ": $err->[0]\n";
    }
}
if ( -e "${d}/a.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "${d}/a.txt";
}
else {
    if ( open my $fh, '>', "${d}/a.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "${d}/a.txt",
          ": $ERRNO\n";
    }
}
if ( -e "${d}/b.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "${d}/b.txt";
}
else {
    if ( open my $fh, '>', "${d}/b.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "${d}/b.txt",
          ": $ERRNO\n";
    }
}
$CHILD_ERROR = chdir("${d}") ? 0 : 1;
# Original bash: ls -1 | grep -v a.txt
my $output_2 = do { open(my $__fh, '-|', 'bash', '-c', 'ls -1 | grep -v a.txt') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_2, "\n");
print(join(" ", grep { length } split /\s+/msx, do { open(my $__fh, '-|', 'bash', '-c', 'ls | grep -v a.txt') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; }), "\n");
$CHILD_ERROR = 0;
$CHILD_ERROR = chdir(q{/}) ? 0 : 1;
if ( -e "${d}" ) {
    if ( -d "${d}" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("${d}", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "${d}", ": $err->[0]\n";
        }
        else {
                    }
    }
    else {
        if ( unlink "${d}" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "${d}",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;


