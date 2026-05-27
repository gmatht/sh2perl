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

print "Hello, World!\n";
print 'Single quoted' . "\n";
$CHILD_ERROR = 0;
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";

exit $main_exit_code;
