#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $MAGIC_30 = 30;
my $MAGIC_25 = 25;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed" . "\n";
print "== Escape sequences ==\n";
print 'bell' . "\n";
print 'backspace' . "\n";
print 'formfeed' . "\n";
print "newline\n" . "\n";
print "carriage\rreturn\n";
print "tab\tseparated\n";
print 'verticaltab' . "\n";
print "== Unicode and hex ==\n";
print 'Hello' . "\n";
print 'Hello' . "\n";
print "== Practical examples ==\n";
printf("%-10s %-10s %s
", "Name", "Age", "City");
printf("%-10s %-10s %s
", "John", "25", "NYC");
printf("%-10s %-10s %s
", "Jane", "30", "LA");

exit $main_exit_code;
