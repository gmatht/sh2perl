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

print "alpha\nbeta\ngamma ...\n";
print "oyster\nsnapper\nsalmon\n";
print "Fin. That is all folks.\n";

exit $main_exit_code;
