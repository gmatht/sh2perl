#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

if ("${0##*/}" eq "autossh-argv0") {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say 'autossh-argv0: This script should not be run like this, see ssh-argv0(1) and autossh(1) for details';
    };
exit 1;
}
# Builtin command 'exec' not implemented
