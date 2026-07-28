#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

# ERR trap not fully supported: echo "Error on line $LINENO"; exit 1
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx'echo "Cleaning up..."; rm -f /tmp/temp_* 2>&1'; print $end_out if $end_out ne q{}; }
say "Traps set up";
