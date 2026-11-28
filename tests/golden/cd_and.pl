#!/usr/bin/env perl
use strict;
use warnings;

if (chdir("/no/such")) { print "x\n";
$? = 0 << 8; } else { warn "$0: line " . __LINE__ . ": cd: " . "/no/such" . ": $!\n"; $? = 1 << 8; }

exit $? >> 8;
