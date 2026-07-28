#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '020_ansi_quoting_basic.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== ANSI-C quoting ==";
say "line1\nline2\tTabbed";
