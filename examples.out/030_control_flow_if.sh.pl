#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

if ((-f"file.txt")) {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
