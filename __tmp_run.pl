#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

my %matrix = ();
my $x;
my $y;
my $z;
$matrix{"0,0"} = eval { int( ($x + $y) * $z ) } // "";
$matrix{"1,1"} = $array[eval { int(${$ENV{index}}) } // ""];
$matrix{"2,2"} = keys %prefix;
$matrix{"3,3"} = scalar(@array);
print "Matrix assignments completed\n";

exit $main_exit_code;
