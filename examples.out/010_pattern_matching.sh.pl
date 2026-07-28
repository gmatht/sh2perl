#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '010_pattern_matching.sh';
my $f1;
my $f2;
my $word;
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
say "== extglob ==";
# extglob option enabled
$f1 = "file.js";
$f2 = "thing.min.js";
if ($f1 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/msx) {
        say 'f1-ok';
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if (!($f2 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/msx)) {
        say 'f2-filtered';
}
say "== nocasematch ==";
# nocasematch option enabled
$word = "Foo";
if ($word =~ /foo/msxi) {
        say 'ci-match';
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
