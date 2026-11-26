#!/usr/bin/env perl
use strict;
use warnings;

if (chdir("/no/such/dir")) {
    $? = 0 << 8;
} else {
    warn "cd: $!\n";
    $? = 1 << 8;
}
if ((0+($? >> 8)) == (0+'0')) {
    print "ok\n";
    $? = 0 << 8;
    $? = 0 << 8;
} else {
    print "bad\n";
    $? = 0 << 8;
    $? = 0 << 8;
}

exit $? >> 8;
