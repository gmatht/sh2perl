#!/usr/bin/env perl
use strict;
use warnings;

print("hi");
$? = 0 << 8;
print "\n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
