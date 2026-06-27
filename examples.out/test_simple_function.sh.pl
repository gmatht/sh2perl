#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
sub get_file_size {
    my $file = "$_[0]";
    my $size = do { my $v = `wc -c < "$file"`; chomp $v; $v };
    print(("File " . $file . " has " . $size . " bytes") . "\n");
}
get_file_size("test_simple_function.sh");