#!/usr/bin/env perl
use strict;
use warnings;

$? = 1 << 8;
if ($? == 0) {
    print "hi\n";
    $? = 0 << 8;
    $? = 0 << 8;
} else {
    $? = 0 << 8;
}

exit $? >> 8;
