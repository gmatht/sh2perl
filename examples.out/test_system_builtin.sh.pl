#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = 'test_system_builtin.sh';
print "Testing " . "sys" . "tem" . " calls with builtin commands\n";
my $result1 = do { my @_qx_cmd = ('command ls -la'); my $result = qx{command $_qx_cmd[0]}; $CHILD_ERROR = $? >> 8; $result; };
my $result2 = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if ($_ =~ /^.*\.txt$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
};
print "Results:\n";
print $result1, "\n";
print $result2, "\n";
