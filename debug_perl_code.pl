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

my $MAGIC_4 = 4;
my $MAGIC_3 = 3;
my $MAGIC_6 = 6;
my $MAGIC_5 = 5;

system 'bash', 'examples/005_args.sh', 'one';
system 'bash', 'examples/005_args.sh', 'one', 'two';
system 'bash', 'examples/005_args.sh', 'one', 'two', 'three';
system 'bash', 'examples/005_args.sh', q{1};
system 'bash', 'examples/005_args.sh', q{1}, q{2}, q{3};
system 'bash', 'examples/005_args.sh', q{1}, 'two', q{3};
system 'bash', 'examples/005_args.sh', "A 'quoted' Sting";
system 'bash', 'examples/005_args.sh', "A 'quoted' Sting", q{2}, q{3}, q{4}, q{5}, q{6};

exit $main_exit_code;
