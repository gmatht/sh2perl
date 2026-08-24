#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $ls_success = 0;
our $CHILD_ERROR;
$ENV{DIR} = do { my $__cs = do {
    my $left_result_0 = do { my $__cs = do { if (chdir('/some/dir')) { $CHILD_ERROR = 0 } else { $CHILD_ERROR = 1 }; q{} }
; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { my $__cs = do { use Cwd; $CHILD_ERROR = 0; getcwd(); }; chomp $__cs; $__cs; };
        $left_result_0 . $right_result_0;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print($ENV{DIR}, "\n");
