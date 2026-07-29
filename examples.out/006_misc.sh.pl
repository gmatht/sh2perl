#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '006_misc.sh';
print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print "inside-subshell\
";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_132 = qx{command echo 'alpha beta' | grep beta};
chomp $output_132;
print $output_132, "\n";
print "exit: ${\($? >> 8)}\n";
