#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/090_nested_if_else.sh';
our $CHILD_ERROR = 0;
my $__nocasematch = 0;

$CHILD_ERROR = ((($__nocasematch) ? lc('hello') eq lc('hello') : 'hello' eq 'hello')) ? 0 : 1;
if ((($__nocasematch) ? lc('hello') eq lc('hello') : 'hello' eq 'hello')) {
    print "greeting\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} elsif ((($__nocasematch) ? lc('hello') eq lc('bye') : 'hello' eq 'bye')) {
    print "farewell\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    print "unknown\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}

exit $? >> 8;
