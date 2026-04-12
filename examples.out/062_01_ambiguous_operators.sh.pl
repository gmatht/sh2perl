#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

print "Testing ambiguous operators...\n";
my $result = 2**3**2;
print "2**3**2 = $result\n";
