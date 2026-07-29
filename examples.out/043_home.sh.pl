#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

my $HOME;

if ($ENV{'HOME'} eq $ENV{'HOME'}) {
        print "1\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\
";
}
if (($ENV{'HOME'} . '/Documents') eq $ENV{'HOME'}) {
        print "2\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\
";
}
if (($ENV{'HOME'} . '/Documents') eq ($ENV{'HOME'} . '/Documents')) {
        print "3\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print "-\
";
}

exit $main_exit_code;

