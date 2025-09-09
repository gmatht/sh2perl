#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );

my $main_exit_code = 0;

my $ls_dir = '.';
my @ls_files = ();
if (opendir my $dh, $ls_dir) {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..};
        push @ls_files, $file;
    }
    closedir $dh;
    @ls_files = sort { $a cmp $b } @ls_files;
}
print join "\n", @ls_files;
