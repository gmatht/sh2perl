#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

say "Testing nested arithmetic...";
my $result = eval { int( (2 + 3) * (4 - 1) + (5 ** 2) ) } // "";
say "Complex arithmetic: $result";
