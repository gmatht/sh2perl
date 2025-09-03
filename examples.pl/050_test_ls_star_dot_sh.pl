#!/usr/bin/env perl

use strict;
use warnings;

print "Testing ls * .sh:\n";
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next if $file =~ /^\.\.?$/;
    print "$file\n" if $file =~ /\.sh$/;
}
closedir($dh);
