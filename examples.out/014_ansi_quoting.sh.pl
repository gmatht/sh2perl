#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '014_ansi_quoting.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== ANSI-C quoting ==";
say "line1\nline2\tTabbed";
say "== Escape sequences ==";
say 'bell';
say 'backspace';
say 'formfeed';
say "newline\n";
say "carriage\rreturn";
say "tab\tseparated";
say 'verticaltab';
say "== Unicode and hex ==";
say 'Hello';
say 'Hello';
say "== Practical examples ==";
printf("%-10s %-10s %s\n", "Name", "Age", "City");
printf("%-10s %-10s %s\n", "John", "25", "NYC");
printf("%-10s %-10s %s\n", "Jane", "30", "LA");
