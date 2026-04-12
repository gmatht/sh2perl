#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

if ($ENV{'HOME'} eq $ENV{'HOME'}) {
        print q{1} . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
}
if (($ENV{'HOME'} . '/Documents') eq $ENV{'HOME'}) {
        print q{2} . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
}
if (($ENV{'HOME'} . '/Documents') eq ($ENV{'HOME'} . '/Documents')) {
        print q{3} . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($CHILD_ERROR != 0) {
        print q{-} . "\n";
}

exit $main_exit_code;
