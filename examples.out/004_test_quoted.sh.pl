#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "Hello, World!\n";
print 'Single quoted' . "\n";
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";

exit $main_exit_code;
