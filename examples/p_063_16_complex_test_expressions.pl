#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
my $dir;
my $file;
my $var;

# TODO(unsupported): test shape ["\"$(wc -l < \"", "$file\")\"", "-gt", "10"]
# TODO(unsupported): test shape ["\"$(wc -l < \"", "$file\")\"", "-gt", "10"]
if ((((($var) ne "") && ((-f ($file)) || (-d ($dir)))) && 0)) {
    say join(' ', "Complex test passed");
}
# 2 construct(s) lowered to TODO markers

