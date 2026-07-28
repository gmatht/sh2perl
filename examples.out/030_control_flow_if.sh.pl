#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$PROGRAM_NAME = '030_control_flow_if.sh';
if ((-f "file.txt")) {
    say "File exists";
}
else {
    say "File does not exist";
}
