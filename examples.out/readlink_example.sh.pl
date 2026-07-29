#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = 'readlink_example.sh';
my $target = do { my @_qx_cmd = ('command readlink -f /usr/bin/vi'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "vi resolves to: $target\n";
my $target2 = do { my @_qx_cmd = ('command readlink -f /usr/bin/python3'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "python3 resolves to: $target2\n";
