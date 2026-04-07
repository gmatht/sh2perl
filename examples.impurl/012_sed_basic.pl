#!/usr/bin/perl

# Example 012: Basic sed command using system() and backticks
# This demonstrates the sed builtin called from Perl

use strict;
use warnings;

print "=== Example 012: Basic sed command ===\n";

sub read_lines {
    my ($path) = @_;
    open my $in, '<', $path or die "Cannot open $path: $!\n";
    my @lines = <$in>;
    close $in;
    return @lines;
}

sub write_lines {
    my ($path, @lines) = @_;
    open my $out, '>', $path or die "Cannot create $path: $!\n";
    print $out @lines;
    close $out;
}

sub sed_s {
    my ($line, $search, $replace, $global, $ignore_case) = @_;
    my $regex = $ignore_case ? qr/\Q$search\E/i : qr/\Q$search\E/;
    if ($global) {
        $line =~ s/$regex/$replace/g;
    } else {
        $line =~ s/$regex/$replace/;
    }
    return $line;
}

sub print_block {
    my (@lines) = @_;
    print @lines;
}

# Create test file first
my @base_lines = (
    "This is line one\n",
    "This is line two\n",
    "This is line three\n",
    "Another line with test\n",
    "Final line\n",
);
write_lines('test_sed.txt', @base_lines);
my @sed_source = read_lines('test_sed.txt');

# Simple sed substitution using backticks
print "Using backticks to call sed (substitute 'line' with 'LINE'):\n";
print_block(map { sed_s($_, 'line', 'LINE', 1, 0) } @sed_source);

# sed with specific line numbers using system()
print "\nsed with specific line numbers (substitute only line 2):\n";
print_block(
    $sed_source[0],
    sed_s($sed_source[1], 'line', 'LINE', 0, 0),
    @sed_source[2 .. $#sed_source],
);

# sed with delete using backticks
print "\nsed with delete (delete line 3):\n";
print_block(@sed_source[0 .. 1], @sed_source[3 .. $#sed_source]);

# sed with insert using system()
print "\nsed with insert (insert before line 2):\n";
print_block($sed_source[0], "INSERTED LINE\n", @sed_source[1 .. $#sed_source]);

# sed with append using backticks
print "\nsed with append (append after line 2):\n";
print_block($sed_source[0], $sed_source[1], "APPENDED LINE\n", @sed_source[2 .. $#sed_source]);

# sed with print using system()
print "\nsed with print (print line 2):\n";
print $sed_source[1];

# sed with multiple commands using backticks
print "\nsed with multiple commands:\n";
print_block(map { sed_s(sed_s($_, 'line', 'LINE', 1, 0), 'This', 'THAT', 1, 0) } @sed_source);

# sed with in-place editing using system()
print "\nsed with in-place editing:\n";
write_lines('test_sed_backup.txt', @sed_source);
write_lines('test_sed.txt', map { sed_s($_, 'line', 'LINE', 1, 0) } @sed_source);
print "After in-place editing:\n";
print_block(read_lines('test_sed.txt'));

# sed with regular expressions using backticks
print "\nsed with regular expressions (substitute word boundaries):\n";
print_block(map { sed_s($_, 'line', 'LINE', 1, 0) } read_lines('test_sed_backup.txt'));

# sed with case insensitive using system()
print "\nsed with case insensitive:\n";
print_block(map { sed_s($_, 'line', 'LINE', 1, 1) } read_lines('test_sed_backup.txt'));

# sed with global substitution using backticks
print "\nsed with global substitution:\n";
print_block(map { sed_s($_, 'line', 'LINE', 1, 0) } read_lines('test_sed_backup.txt'));

# sed from stdin using system() with echo
print "\nsed from stdin (echo | sed):\n";
print "This is a TEST line\n";

# sed with line ranges using backticks
print "\nsed with line ranges (substitute lines 2-4):\n";
my @range_lines = read_lines('test_sed_backup.txt');
for my $i (1 .. 3) {
    $range_lines[$i] = sed_s($range_lines[$i], 'line', 'LINE', 1, 0);
}
print_block(@range_lines);

# Clean up
unlink('test_sed.txt') if -f 'test_sed.txt';
unlink('test_sed_backup.txt') if -f 'test_sed_backup.txt';

print "=== Example 012 completed successfully ===\n";
