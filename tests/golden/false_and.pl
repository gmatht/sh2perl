#!/usr/bin/env perl
use strict;
use warnings;
our $CHILD_ERROR = 0;

$? = 1 << 8; $CHILD_ERROR = $? >> 8;
if ($? == 0) {
    print "hi\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
$CHILD_ERROR = $? >> 8;
}

exit $? >> 8;
