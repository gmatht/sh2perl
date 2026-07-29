#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);

my $i;

if (-f "file.txt") {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
for my $i (1..5) {
    print "Number: $i\n";
}
$i = 5;
while ($i < 10) {
    print "Counter: $i\n";
    $i = int($i + 1);
}

sub greet {
    my ($file) = @_;
    print "Hello, $_[0]!\n";
    return;
}
greet("World");

