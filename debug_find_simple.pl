#!/usr/bin/env perl
use strict;
use warnings;
use File::Find;

my @results;
my $start_path = q{.};

find(sub {
    my $file = $File::Find::name;
    my $filename = $_;
    
    # Debug output for .sh files
    if ($filename =~ /\.sh$/) {
        print "Found .sh file: $file (filename: $filename)\n";
        
        if (!-f $file) {
            print "  -> Not a regular file, skipping\n";
            return;
        }
        
        if ($filename !~ /.*[.]sh$/msx) {
            print "  -> Pattern doesn't match, skipping\n";
            return;
        }
        
        print "  -> Adding to results\n";
        push @results, $file;
    }
}, $start_path);

print "\nFinal results:\n";
for my $result (@results) {
    print "$result\n";
}