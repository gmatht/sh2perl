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

my $MAGIC_25 = 25;
my $MAGIC_30 = 30;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed" . "\n";
$CHILD_ERROR = 0;
print "== Escape sequences ==\n";
print 'bell' . "\n";
$CHILD_ERROR = 0;
print 'backspace' . "\n";
$CHILD_ERROR = 0;
print 'formfeed' . "\n";
$CHILD_ERROR = 0;
print "newline\n" . "\n";
$CHILD_ERROR = 0;
print "carriage\rreturn\n";
print "tab\tseparated\n";
print 'verticaltab' . "\n";
$CHILD_ERROR = 0;
print "== Unicode and hex ==\n";
print 'Hello' . "\n";
$CHILD_ERROR = 0;
print 'Hello' . "\n";
$CHILD_ERROR = 0;
print "== Practical examples ==\n";
printf("%-10s %-10s %s\n", "Name", "Age", "City");
printf("%-10s %-10s %s\n", "John", "25", "NYC");
printf("%-10s %-10s %s\n", "Jane", "30", "LA");

exit $main_exit_code;
