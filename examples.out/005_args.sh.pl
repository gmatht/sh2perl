#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = '005_args.sh';
print "== Argument count ==\n";
print(scalar(@ARGV), "\n");
print "== Arguments ==\n";
for my $a (@ARGV) {
    print "Arg: ${a}\n";
}
