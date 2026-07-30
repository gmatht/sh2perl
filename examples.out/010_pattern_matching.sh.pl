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

$0 = '010_pattern_matching.sh';
my $f2;
my $word;
my $s;
my $f1;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== [[ pattern and regex ]]\n";
$s = "file.txt";
if ($s =~ /^.*[.]txt$/ms) {
        print "pattern-match\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($s =~ /^file[.][a-z]+$/m) {
        print "regex-match\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
print "== extglob ==\n";
# extglob option enabled
$f1 = "file.js";
$f2 = "thing.min.js";
if ($f1 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/ms) {
        print "f1-ok\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if (!($f2 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/ms)) {
        print "f2-filtered\n";
}
print "== nocasematch ==\n";
# nocasematch option enabled
$word = "Foo";
if ($word =~ /foo/i) {
        print "ci-match\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
