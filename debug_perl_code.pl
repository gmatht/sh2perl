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

$PROGRAM_NAME = '049_local.sh';
my $a = q{1};
print $a;
if ( !( ($a) =~ m{\n\z}msx ) ) { print "\n"; }
do {
    local %ENV = %ENV;
    my $a;
        $a = q{2};
        print $a;
if ( !( ($a) =~ m{\n\z}msx ) ) { print "\n"; }
    q{};
};
do {
    local %ENV = %ENV;
    my $a;
    print $a;
if ( !( ($a) =~ m{\n\z}msx ) ) { print "\n"; }
    q{};
};
print $a;
if ( !( ($a) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;
