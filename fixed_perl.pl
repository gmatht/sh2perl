#!/usr/bin/env perl
use strict;
use warnings;
use File::Find;

my $found_files = do {
    my @results;
    my $start_path = q{.};
    find(sub {
        my $file = $File::Find::name;
        my $filename = $_;
        if (!-f $file) {
            return;
        }
        if ($filename !~ /.*[.]sh$/msx) {
            return;
        }
        push @results, $file;
    }, $start_path);
    join "\n", @results;
};

print "Found shell scripts:\n";
print $found_files, "\n";
