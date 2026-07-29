#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
our $CHILD_ERROR;
if ((defined $ENV{ZSH_VERSION} && $ENV{ZSH_VERSION} ne q{} ? $ENV{ZSH_VERSION} : q{}) ne q{}) {
    print "zsh\n";
}
else {
    if ((defined $ENV{BASH_VERSION} && $ENV{BASH_VERSION} ne q{} ? $ENV{BASH_VERSION} : q{}) ne q{}) {
        print "bash\n";
    }
}
