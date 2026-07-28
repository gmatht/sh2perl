#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '001_simple.sh';
say "Hello, World!";
if ((-f "test.txt")) {
    say "File exists";
}
my $i;
for my $i ( 1 .. $MAX_LOOP_5 ) {
    say $i;
}
