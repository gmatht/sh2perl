#!/usr/bin/env perl
use strict;
use warnings;
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '043_home.sh';
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
