#!/usr/bin/env perl

use strict;
use warnings;

# Practical brace expansion examples

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
