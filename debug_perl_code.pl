#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '063_06_complex_pipeline_background.sh';
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    exec 'bash', '-c', '(echo Starting; sleep 1)';
    croak "exec failed: $OS_ERROR\n";
} else {
    die "Cannot fork: $ERRNO\n";
}
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    exec 'bash', '-c', '(echo Processing; sleep 2)';
    croak "exec failed: $OS_ERROR\n";
} else {
    die "Cannot fork: $ERRNO\n";
}
1 while wait() > -1;
$CHILD_ERROR = $? >> 8;
print "All done\n";

exit $main_exit_code;
