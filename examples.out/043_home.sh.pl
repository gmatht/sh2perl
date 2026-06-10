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

$PROGRAM_NAME = '043_home.sh';
if ($ENV{'HOME'} eq $ENV{'HOME'}) {
        print q{1} . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
    $CHILD_ERROR = 0;
}
if (($ENV{'HOME'} . '/Documents') eq $ENV{'HOME'}) {
        print q{2} . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
    $CHILD_ERROR = 0;
}
if (($ENV{'HOME'} . '/Documents') eq ($ENV{'HOME'} . '/Documents')) {
        print q{3} . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
    $CHILD_ERROR = 0;
}

exit $main_exit_code;
