#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== ANSI-C quoting ==\n");
print("line1\nline2\tTabbed\n");
print("== Escape sequences ==\n");
print("bell\a\n");
print("backspace\b\n");
print("formfeed\f\n");
print("newline\n\n");
print("returnge\n");
print("tab\tseparated\n");
print("vertical
        tab\n");
print("== Unicode and hex ==\n");
print("Hello\n");
print("Hello\n");
print("== Practical examples ==\n");
printf("%-10s %-10s %s\n", "Name", "Age", "City");
printf("%-10s %-10s %s\n", "John", "25", "NYC");
printf("%-10s %-10s %s\n", "Jane", "30", "LA");