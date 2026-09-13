#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = 'parse-brace-close.sh';
print "1 2 3 4 5\n";
print "file.txt file.bak\n";
for my $i (1..3) {
    print($i, "\n");
}
