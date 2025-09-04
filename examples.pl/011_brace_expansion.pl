#!/usr/bin/env perl

use strict;
use warnings;

# Brace expansion examples
# Demonstrates various expansion patterns in Perl

print "== Basic expansion ==\n";
print join(" ", 1..5) . "\n";
print join(" ", 'a'..'c') . "\n";
print join(" ", map { sprintf("%02d", $_) } 0, 2, 4) . "\n";

print "== Advanced expansion ==\n";
my @letters = qw(a b c);
my @numbers = qw(1 2 3);
for my $letter (@letters) {
    for my $number (@numbers) {
        print "$letter$number ";
    }
}
print "\n";

print join(" ", map { $_ * 2 - 1 } 1..5) . "\n";  # 1..10..2 equivalent
print join(" ", map { chr(ord('a') + $_ * 3) } 0..8) . "\n";  # a..z..3 equivalent

print "== Practical examples ==\n";
# Create numbered files
for my $i (1..5) {
    my $filename = sprintf("file_%03d.txt", $i);
    open(my $fh, '>', $filename) or die "Cannot create $filename: $!\n";
    close($fh);
}

# List files
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    print "$file\n" if $file =~ /^file_.*\.txt$/;
}
closedir($dh);

# Remove files
for my $i (1..5) {
    my $filename = sprintf("file_%03d.txt", $i);
    unlink($filename) if -f $filename;
}

