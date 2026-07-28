#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

say "Testing complex here-documents...";
print q{This is a test line
This is not a test line
This is another test line
};
