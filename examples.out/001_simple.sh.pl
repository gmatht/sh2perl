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

my $MAX_LOOP_5 = 5;

print "Hello, World!\n";
if ((-f"test.txt")) {
    print "File exists\n";
}
my $i;
for my $i ( 1 .. $MAX_LOOP_5 ) {
    print $i;
if ( !( $i =~ m{\n\z}msx ) ) { print "\n"; }
}

exit $main_exit_code;
