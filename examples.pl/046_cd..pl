#!/usr/bin/env perl

use strict;
use warnings;

# Change directory and list
chdir('..') or die "Cannot change directory: $!\n";
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next if $file =~ /^\.\.?$/;
    print "$file\n";
}
closedir($dh);

