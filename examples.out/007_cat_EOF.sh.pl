#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '007_cat_EOF.sh';
print "alpha
beta
gamma ...
";
print "oyster
snapper
salmon
";
say "Fin. That is all folks.";
