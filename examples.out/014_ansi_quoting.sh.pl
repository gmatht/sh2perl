#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $__set_e = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '014_ansi_quoting.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed\n";
print "== Escape sequences ==\n";
print "bell\n";
print "backspace\n";
print "formfeed\n";
print "newline\n\n";
print "carriage\rreturn\n";
print "tab\tseparated\n";
print "verticaltab\n";
print "== Unicode and hex ==\n";
print "Hello\n";
print "Hello\n";
print "== Practical examples ==\n";
printf("%-10s %-10s %s\n", "Name", "Age", "City");
printf("%-10s %-10s %s\n", "John", "25", "NYC");
printf("%-10s %-10s %s\n", "Jane", "30", "LA");
