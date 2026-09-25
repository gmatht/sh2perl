#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = 'readlink_relative.sh';
my $relative = do { my $__cs = do { use Cwd qw(abs_path); my $_r = abs_path('/usr/bin/corepack'); defined $_r ? $_r : q{}; }; chomp $__cs; $__cs; };
print "Corepack resolves to: ${relative}\n";
