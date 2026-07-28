#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$main_exit_code = system('.', './setup-vars') >> 8;
$CHILD_ERROR = 0;
my $returncode = $?;
$main_exit_code = system('.', './report-yesno') >> 8;
