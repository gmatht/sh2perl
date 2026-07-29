#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = 'readlink_flags.sh';
my $existing = do { my @_qx_cmd = ('command readlink -e /usr/bin/vi'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
my $missing = do { my @_qx_cmd = ('command readlink -m /nonexistent/path'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
my $full = do { my @_qx_cmd = ('command readlink -f /usr/bin/python3'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "Existing: $existing\n";
print "Missing:  $missing\n";
print "Full:     $full\n";
