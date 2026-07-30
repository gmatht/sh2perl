#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = '030_control_flow_if.sh';
if (-f "file.txt") {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
print("exit: " . ($? >> 8), "\n");
