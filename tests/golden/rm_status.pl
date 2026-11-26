#!/usr/bin/env perl
use strict;
use warnings;

do { my $__ok = 1; for my $__p ('/no/such/file') { $__ok = 0 unless unlink($__p); } $? = ($__ok ? 0 : 1) << 8; };
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
