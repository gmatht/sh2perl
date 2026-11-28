#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'tests/tier1/dup_stderr_to_stdout.sh';
our $CHILD_ERROR = 0;


use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

{
    open(my $__sh2_dup0, '>&', \*STDERR) or die;
    open(STDERR, '>&', \*STDOUT) or die;
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dup0) or die;
}
say "rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (do {
open(my $__sh2_duperr1, '>&', \*STDERR) or die; open(STDERR, '>&', \*STDOUT) or die; 
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
open(STDERR, '>&', $__sh2_duperr1) or die; 
($CHILD_ERROR == 0)
}) {
    print "taken\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "else-branch\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
print "done\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;

exit $? >> 8;
