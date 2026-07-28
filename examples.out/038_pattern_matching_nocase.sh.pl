#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '038_pattern_matching_nocase.sh';
my $word;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== nocasematch ==";
# nocasematch option enabled
$word = "Foo";
if ($word =~ /^foo$/msxi) {
        say 'ci-match';
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
