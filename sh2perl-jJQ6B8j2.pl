#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Escape sequences ==";
say 'bell';
say 'backspace';
say 'formfeed';
say "newline\n";
say "carriage\rreturn";
say "tab\tseparated";
say 'verticaltab';
