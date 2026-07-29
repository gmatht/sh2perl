#!/usr/bin/env perl
use strict;
use warnings;
my $found_files = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if (-f $_ && $_ =~ /^.*\.sh$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
};
print "Found shell scripts:\n";
print $found_files, "\n";

