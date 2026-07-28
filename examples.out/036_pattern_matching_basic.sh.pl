#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '036_pattern_matching_basic.sh';
my $s;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== [[ pattern and regex ]]";
$s = "file.txt";
if ($s =~ /^.*[.]txt$/msx) {
        say 'pattern-match';
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($s =~ /^file[.][a-z]+$/msx) {
        say 'regex-match';
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
