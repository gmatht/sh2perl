#!/usr/bin/perl
use strict;
use warnings;
use Perl::Critic;

my $critic = Perl::Critic->new(
    -severity => 1,  # brutal
);

my $file = $ARGV[-1];  # Last argument is the file
my @violations = $critic->critique($file);

if (@violations) {
    print "Perl::Critic violations found:\n";
    foreach my $violation (@violations) {
        print $violation->description() . "\n";
    }
    exit 1;
} else {
    print "No violations found.\n";
    exit 0;
}
