#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '004_test_quoted.sh';
say "Hello, World!";
say 'Single quoted';
say "String with \"escaped\" quotes";
say "String with 'single' quotes";
