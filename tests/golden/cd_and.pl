#!/usr/bin/env perl
use strict;
use warnings;

if (chdir("/no/such")) { print "x\n";
$? = 0 << 8; } else { warn "cd: $!\n"; $? = 1 << 8; }

exit $? >> 8;
