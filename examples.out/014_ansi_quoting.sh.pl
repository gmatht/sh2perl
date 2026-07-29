#!/usr/bin/env perl
use strict;
use warnings;
my $output         = q{};
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed\n";
print "== Escape sequences ==\n";
print "bell\
";
print "backspace\
";
print "formfeed\
";
print "newline\n\n";
print "carriage\rreturn\n";
print "tab\tseparated\n";
print "verticaltab\
";
print "== Unicode and hex ==\n";
print "Hello\
";
print "Hello\
";
print "== Practical examples ==\n";
printf("%-10s %-10s %s\n", "Name", "Age", "City");
printf("%-10s %-10s %s\n", "John", "25", "NYC");
printf("%-10s %-10s %s\n", "Jane", "30", "LA");

