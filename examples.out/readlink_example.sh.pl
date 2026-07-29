#!/usr/bin/env perl
use strict;
use warnings;
my $target = do { my @_qx_cmd = ('command readlink -f "$1"'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "Canonical path: $target\n";

