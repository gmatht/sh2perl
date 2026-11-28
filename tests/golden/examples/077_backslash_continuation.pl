#!/usr/bin/env perl
use strict;
use warnings;

do { my $__t = 'hello' . "\n"; $__t =~ tr/a-z/A-Z/; print $__t; $? = 0 << 8; };
print("exit: " . (($? >> 8)), "\n");
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
