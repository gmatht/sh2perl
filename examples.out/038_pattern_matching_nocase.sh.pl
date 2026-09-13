#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $__set_e = 0;
my $output = '';
our $CHILD_ERROR;

$0 = '038_pattern_matching_nocase.sh';
my $word;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== nocasematch ==\n";
# nocasematch option enabled
$word = "Foo";
if ($word =~ /^foo$/mi) {
        print "ci-match\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
