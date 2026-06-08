#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== [[ pattern and regex ]]\n";
my $s;
$s = "file.txt";
if ($s =~ /^.*[.]txt$/msx) {
        print 'pattern-match' . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($s =~ /^file[.][a-z]+$/msx) {
        print 'regex-match' . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;
