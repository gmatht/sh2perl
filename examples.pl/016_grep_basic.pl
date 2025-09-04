#!/usr/bin/env perl

use strict;
use warnings;

# Basic grep usage examples
# Demonstrates fundamental pattern matching operations

# Basic usage
if (!open(my $fh, '<', '/dev/null')) {
    print "No matches found\n";
}

# Case-insensitive search
my $text = "HELLO world";
if ($text =~ /hello/i) {
    print "$text\n";
}

# Invert match (lines NOT matching)
my @lines = qw(line1 line2 line3);
for my $line (@lines) {
    print "$line\n" unless $line =~ /line2/;
}

# Show line numbers
@lines = qw(first second third);
for my $i (0..$#lines) {
    if ($lines[$i] =~ /second/) {
        print ($i + 1) . ":$lines[$i]\n";
    }
}

# Count matching lines only
@lines = qw(match no\ match match\ again);
my $count = 0;
for my $line (@lines) {
    $count++ if $line =~ /match/;
}
print "$count\n";

# Only print the matching part of the line
$text = "text with pattern123 in it";
if ($text =~ /(pattern\d+)/) {
    print "$1\n";
}

