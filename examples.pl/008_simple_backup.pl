#!/usr/bin/env perl

use strict;
use warnings;

# Simple Perl script example
print "Hello, World!\n";

# TODO: Support multi-column output
# ls -1 | grep -v __tmp_test_output.pl
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next if $file =~ /^\.\.?$/;  # Skip . and ..
    next if $file eq '__tmp_test_output.pl';
    print "$file\n";
}
closedir($dh);

# This should be a single token, not two.
# AST_MUST_CONTAIN: [Literal("-1")]
my $ls_output = `ls | grep -v __tmp_test_output.pl`;
print $ls_output;

# Let's not consider ls -la at the moment as permissions are OS dependent
# ls -la
# grep "pattern" file.txt

