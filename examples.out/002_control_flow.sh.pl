#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$PROGRAM_NAME = '002_control_flow.sh';
my $i;

if ((-f "file.txt")) {
    say "File exists";
}
else {
    say "File does not exist";
}
for my $i ( 1 .. $MAX_LOOP_5 ) {
    say "Number: $i";
}
$i = 5;
while ( $i < 10 ) {
    say "Counter: $i";
    $i = eval { int($i + 1) } // "";
}

sub greet {
    my ($file) = @_;
    say "Hello, $_[0]!";
    return;
}
greet("World");
