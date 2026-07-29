#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/025_param_expansion_file.txt";
print basename(${path}), "\n";
print dirname(${path}), "\n";
my $s2 = "abba";
print $s2 =~ s/b/X/grs, "\n";

