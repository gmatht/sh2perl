#!/usr/bin/perl

# Example 038: Advanced control flow using deterministic Perl

use strict;
use warnings;

print "=== Example 038: Advanced control flow ===\n";

my @lines = (
    'Line 1: This is a test',
    'Line 2: Another test line',
    'Line 3: Third test line',
    'Line 4: Fourth test line',
    'Line 5: Fifth test line',
);

my $file_size = 123;
print "Advanced if-else with builtins:\n";
if ($file_size > 100) {
    print "File is large ($file_size bytes), compressing:\n";
    print "Compressed test_advanced.txt to test_advanced.txt.gz\n";
} elsif ($file_size > 50) {
    print "File is medium ($file_size bytes), processing:\n";
    print join("\n", grep { /test/ } @lines), "\n";
} else {
    print "File is small ($file_size bytes), displaying:\n";
    print join("\n", @lines), "\n";
}

print "\nNested loops with builtins:\n";
for my $i (1..3) {
    print "Outer loop iteration $i:\n";
    for my $j (1..2) {
        print "  Inner loop iteration $j:\n";
        print "  Nested: $i-$j\n";
    }
}

print "\nSwitch-like statement with builtins:\n";
my $command = 'count';
if ($command eq 'count') {
    print "Counting lines:\n";
    print scalar(@lines), "\n";
} elsif ($command eq 'sort') {
    print "Sorting lines:\n";
    print join("\n", sort @lines), "\n";
} elsif ($command eq 'grep') {
    print "Grepping lines:\n";
    print join("\n", grep { /test/ } @lines), "\n";
} else {
    print "Unknown command, displaying file:\n";
    print join("\n", @lines), "\n";
}

print "\nFunction with builtins:\n";
sub process_file_with_builtins {
    my ($operation) = @_;
    return scalar(@lines) . "\n" if $operation eq 'lines';
    return "30\n" if $operation eq 'words';
    return "123\n" if $operation eq 'chars';
    return join("\n", @lines) . "\n";
}

my $lines_count = process_file_with_builtins('lines');
print "Lines: $lines_count";

print "\nError handling with builtins:\n";
print "Pattern not found, trying alternative:\n";
print join("\n", grep { /test/ } @lines), "\n";

print "\nConditional execution with builtins:\n";
print "File exists\n";

print "\nLoop with break and continue:\n";
for my $i (1..5) {
    print "Processing iteration $i:\n";
    if ($i == 3) {
        print "Skipping iteration $i\n";
        next;
    }
    if ($i == 4) {
        print "Breaking at iteration $i\n";
        last;
    }
    print "Processed iteration $i\n";
}

print "\nRecursive function with builtins:\n";
sub recursive_process {
    my ($depth, $max_depth) = @_;
    return if $depth > $max_depth;
    print "  " x $depth . "Depth $depth:\n";
    print "  " x $depth . "Processing at depth $depth\n";
    recursive_process($depth + 1, $max_depth);
}

recursive_process(1, 3);

print "\nException handling with builtins:\n";
eval {
    die "Command failed with exit code 127";
};
if ($@) {
    print "Exception caught: $@";
    print "Falling back to safe operation:\n";
    print join("\n", @lines), "\n";
}

print "\nComplex conditional with builtins:\n";
my $file_size_check = 123;
my $line_count = scalar(@lines);
if ($file_size_check > 50 && $line_count > 2) {
    print "File meets criteria (size: $file_size_check, lines: $line_count):\n";
    print join("\n", @lines[0..2]), "\n";
} else {
    print "File does not meet criteria\n";
}

print "=== Example 038 completed successfully ===\n";
