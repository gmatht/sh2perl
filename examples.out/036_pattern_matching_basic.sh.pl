#!/usr/bin/env perl
use strict;
use warnings;
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $s;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== [[ pattern and regex ]]\n";
$s = "file.txt";
if ($s =~ /^.*[.]txt$/ms) {
        print "pattern-match\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($s =~ /^file[.][a-z]+$/m) {
        print "regex-match\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;

