#!/usr/bin/env perl

use strict;
use warnings;

# Grep context and file operation examples
# Demonstrates context and file handling capabilities

# Context lines: after, before, and both
my @lines = qw(line1 line2 TARGET line4 line5);
my $target_index = -1;
for my $i (0..$#lines) {
    if ($lines[$i] eq 'TARGET') {
        $target_index = $i;
        last;
    }
}

if ($target_index >= 0) {
    # After context (A 2)
    print "After context:\n";
    for my $i ($target_index..min($target_index + 2, $#lines)) {
        print "$lines[$i]\n";
    }
    
    # Before context (B 2)
    print "Before context:\n";
    for my $i (max(0, $target_index - 2)..$target_index) {
        print "$lines[$i]\n";
    }
    
    # Both context (C 1)
    print "Both context:\n";
    for my $i (max(0, $target_index - 1)..min($target_index + 1, $#lines)) {
        print "$lines[$i]\n";
    }
}

# Recursive search in current directory
print "Creating test files...\n";
open(my $fh, '>', "temp_file1.txt") or die "Cannot create temp_file1.txt: $!\n";
print $fh "pattern in file1\n";
close($fh);

open($fh, '>', "temp_file2.txt") or die "Cannot create temp_file2.txt: $!\n";
print $fh "no pattern in file2\n";
close($fh);

open($fh, '>', "temp_file3.txt") or die "Cannot create temp_file3.txt: $!\n";
print $fh "pattern in file3\n";
close($fh);

print "Recursive search results:\n";
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next unless $file =~ /\.txt$/;
    open($fh, '<', $file) or next;
    while (my $line = <$fh>) {
        if ($line =~ /pattern/) {
            print "$file:$line";
        }
    }
    close($fh);
}
closedir($dh);

print "Result 2...\n";
# Print file names with matches
opendir($dh, '.') or die "Cannot open directory: $!\n";
my @matching_files;
while (my $file = readdir($dh)) {
    next unless $file =~ /\.txt$/;
    open($fh, '<', $file) or next;
    my $has_match = 0;
    while (my $line = <$fh>) {
        if ($line =~ /pattern/) {
            $has_match = 1;
            last;
        }
    }
    close($fh);
    push @matching_files, $file if $has_match;
}
closedir($dh);
@matching_files = sort @matching_files;
print join("\n", @matching_files) . "\n";

print "Result 3...\n";
# Print file names without matches
opendir($dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next unless $file =~ /\.txt$/;
    open($fh, '<', $file) or next;
    my $has_match = 0;
    while (my $line = <$fh>) {
        if ($line =~ /pattern/) {
            $has_match = 1;
            last;
        }
    }
    close($fh);
    print "$file\n" unless $has_match;
}
closedir($dh);

# Cleanup
unlink("temp_file1.txt", "temp_file2.txt", "temp_file3.txt");

sub min { $_[0] < $_[1] ? $_[0] : $_[1] }
sub max { $_[0] > $_[1] ? $_[0] : $_[1] }

