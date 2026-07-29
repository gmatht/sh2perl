#!/usr/bin/env perl
use strict;
use warnings;

$PROGRAM_NAME = '002_control_flow.sh';
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
