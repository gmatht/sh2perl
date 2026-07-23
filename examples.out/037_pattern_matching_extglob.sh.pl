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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '037_pattern_matching_extglob.sh';
my $f1;
my @f1;
my %f1;
my $f2;
my @f2;
my %f2;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== extglob ==\n";
# extglob option enabled
$f1 = "file.js";
$f2 = "thing.min.js";
if ($f1 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/msx) {
        print 'f1-ok' . "\n";
    $CHILD_ERROR = 0;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if (!($f2 =~ /^(?!.*.*[.]min[.]js$).*[.]js$/msx)) {
        print 'f2-filtered' . "\n";
    $CHILD_ERROR = 0;
}

exit $main_exit_code;
