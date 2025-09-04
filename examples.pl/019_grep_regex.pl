#!/usr/bin/env perl

use strict;
use warnings;

# Grep regex and pattern matching examples
# Demonstrates advanced pattern matching capabilities

# Extended regular expressions (ERE)
my $text = "foo123 bar456";
if ($text =~ /(foo|bar)\d+/) {
    print "$text\n";
}

# Fixed strings (no regex)
$text = "a+b*c?";
if ($text =~ /\Qa+b*c?\E/) {
    print "$text\n";
}

# Match whole words
$text = "word wordly subword";
if ($text =~ /\bword\b/) {
    print "$text\n";
}

# Match whole lines
my @lines = ("exact whole line", "partial line");
for my $line (@lines) {
    if ($line eq "exact whole line") {
        print "$line\n";
    }
}

# Multiple patterns
@lines = ("error message", "warning message", "info message");
for my $line (@lines) {
    if ($line =~ /error|warning/) {
        print "$line\n";
    }
}

# Read patterns from here-string
my @patterns = ("error", "warning");
@lines = ("error", "warning");
for my $line (@lines) {
    for my $pattern (@patterns) {
        if ($line =~ /$pattern/) {
            print "$line\n";
            last;
        }
    }
}

# Complex regex with groups
$text = "file123.txt backup456.bak";
if ($text =~ /([a-z]+)(\d+)\.([a-z]+)/) {
    print "$text\n";
}

