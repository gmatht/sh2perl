#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = '004_test_quoted.sh';
print "Hello, World!\n";
print "Single quoted\n";
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";
print("exit: " . ($? >> 8), "\n");
