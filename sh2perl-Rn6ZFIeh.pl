#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$main_exit_code = system('.', './setup-vars') >> 8;
$main_exit_code = system('.', './setup-tempfile') >> 8;
do {
local *STDERR;
open STDERR, '>', $tempfile or croak "Cannot access file: $OS_ERROR\n";
    $CHILD_ERROR = 0;
};
my $returncode = $?;
$main_exit_code = system('.', './report-tempfile') >> 8;
