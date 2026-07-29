#!/usr/bin/env perl
use strict;
use warnings;
my $relative = do { my @_qx_cmd = ('command readlink -f /usr/bin/corepack'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "Corepack resolves to: $relative\n";

