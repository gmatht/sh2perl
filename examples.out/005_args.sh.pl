#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '005_args.sh';
say "== Argument count ==";
say scalar(@ARGV);
say "== Arguments ==";
my $a;
for my $a (@ARGV) {
    say "Arg: $a";
}
