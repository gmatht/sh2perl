#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $ls_success     = 0;
my $output         = q{};
our $CHILD_ERROR;

print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
my $output_0 = qx{command ls -1 | grep -v __tmp_test_output.pl};
chomp $output_0;
print $output_0, "\n";
print join(" ", grep { length } split /\s+/msx, do { chomp(my $_r = qx{command ls | grep -v __tmp_test_output.pl}); $_r; });

