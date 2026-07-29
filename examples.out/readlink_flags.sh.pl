#!/usr/bin/env perl
use strict;
use warnings;
my $target_e = do { my @_qx_cmd = ('command readlink -e "$1"'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
my $target_m = do { my @_qx_cmd = ('command readlink -m "$1"'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
my $target_f = do { my @_qx_cmd = ('command readlink -f "$1"'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "Canonical (existing): $target_e\n";
print "Canonical (missing):  $target_m\n";
print "Canonical (full):     $target_f\n";

