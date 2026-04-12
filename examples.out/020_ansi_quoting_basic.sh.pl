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

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed" . "\n";
