#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '037_pattern_matching_extglob.sh';
my $f1;
my $f2;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
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

exit $main_exit_code;
