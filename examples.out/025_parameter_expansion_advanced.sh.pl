#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/file.txt";
do {
    my $output = basename(${path});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = dirname(${path});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $s2;
$s2 = "abba";
print $s2 =~ s/b/X/grs;
if ( !( ($s2 =~ s/b/X/grs) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;
