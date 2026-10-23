#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = '001_simple.sh';
print "Hello, World!\n";
if (-f "test.txt") {
    print "File exists\n";
}
for my $i (1..5) {
    print($i, "\n");
}
