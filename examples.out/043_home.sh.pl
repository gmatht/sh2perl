#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '043_home.sh';
if ($ENV{'HOME'} eq $ENV{'HOME'}) {
        print "1\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\n";
}
if (($ENV{'HOME'} . '/Documents') eq $ENV{'HOME'}) {
        print "2\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\n";
}
if (($ENV{'HOME'} . '/Documents') eq ($ENV{'HOME'} . '/Documents')) {
        print "3\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\n";
}

exit $main_exit_code;
