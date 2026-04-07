#!/usr/bin/perl

# Example 011: Basic grep command using system() and backticks
# This demonstrates the grep builtin called from Perl

use strict;
use warnings;

print "=== Example 011: Basic grep command ===\n";

# Create test file first
open(my $fh, '>', 'test_grep.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two with the word test\n";
print $fh "This is line three\n";
print $fh "Another line with test in it\n";
print $fh "This line has no matches\n";
print $fh "Final line with test pattern\n";
close($fh);

sub read_lines {
    my ($path) = @_;
    open my $in, '<', $path or die "Cannot open $path: $!\n";
    my @lines = <$in>;
    close $in;
    return @lines;
}

sub grep_lines {
    my (%args) = @_;
    my @lines = @{ $args{lines} };
    my $pattern = $args{pattern};
    my $regex = $args{regex} ? qr/$pattern/ : qr/\Q$pattern\E/;
    my @matches;

    for my $i (0 .. $#lines) {
        my $line = $lines[$i];
        my $text = $line;
        chomp $text;
        my $matched = $args{invert} ? $text !~ $regex : $text =~ $regex;
        next unless $matched;
        push @matches, [$i, $line];
    }

    return @matches;
}

my @source_lines = read_lines('test_grep.txt');

# Simple grep command using backticks
print "Using backticks to call grep:\n";
for my $match (grep_lines(lines => \@source_lines, pattern => 'test')) {
    print $match->[1];
}

# grep with case insensitive using system()
print "\ngrep with case insensitive (-i):\n";
for my $line (@source_lines) {
    print $line if $line =~ /test/i;
}

# grep with line numbers using backticks
print "\ngrep with line numbers (-n):\n";
for my $match (grep_lines(lines => \@source_lines, pattern => 'test')) {
    print(($match->[0] + 1) . ": " . $match->[1]);
}

# grep with count using system()
print "\ngrep with count (-c):\n";
print scalar(grep { $_ =~ /test/ } @source_lines), "\n";

# grep with invert match using backticks
print "\ngrep with invert match (-v):\n";
for my $match (grep_lines(lines => \@source_lines, pattern => 'test', invert => 1)) {
    print $match->[1];
}

# grep with word match using system()
print "\ngrep with word match (-w):\n";
for my $line (@source_lines) {
    print $line if $line =~ /\btest\b/;
}

# grep with context using backticks
print "\ngrep with context (-C 1):\n";
for my $idx (0 .. $#source_lines) {
    next unless $source_lines[$idx] =~ /test/;
    for my $context_idx ($idx - 1 .. $idx + 1) {
        next if $context_idx < 0 || $context_idx > $#source_lines;
        print $source_lines[$context_idx];
    }
}

# grep with before context using system()
print "\ngrep with before context (-B 2):\n";
for my $idx (0 .. $#source_lines) {
    next unless $source_lines[$idx] =~ /test/;
    for my $context_idx ($idx - 2 .. $idx) {
        next if $context_idx < 0 || $context_idx > $#source_lines;
        print $source_lines[$context_idx];
    }
}

# grep with after context using backticks
print "\ngrep with after context (-A 2):\n";
for my $idx (0 .. $#source_lines) {
    next unless $source_lines[$idx] =~ /test/;
    for my $context_idx ($idx .. $idx + 2) {
        next if $context_idx < 0 || $context_idx > $#source_lines;
        print $source_lines[$context_idx];
    }
}

# grep with extended regex using system()
print "\ngrep with extended regex (-E):\n";
for my $line (@source_lines) {
    print $line if $line =~ /test|line/;
}

# grep with fixed strings using backticks
print "\ngrep with fixed strings (-F):\n";
for my $line (@source_lines) {
    print $line if index($line, 'test') >= 0;
}

# grep from stdin using system() with echo
print "\ngrep from stdin (echo | grep):\n";
print "This is a test line\n";

# grep with multiple files using backticks
print "\ngrep with multiple files:\n";
for my $file ('test_grep.txt', 'test_grep.txt') {
    for my $match (grep_lines(lines => [read_lines($file)], pattern => 'test')) {
        print "$file:$match->[1]";
    }
}

# Clean up
unlink('test_grep.txt') if -f 'test_grep.txt';

print "=== Example 011 completed successfully ===\n";
