#!/usr/bin/env perl

use strict;
use warnings;

# Background process equivalent
my $pid = fork();
if ($pid == 0) {
    # Child process
    sleep(1);
    print "a\n";
    exit(0);
} elsif ($pid > 0) {
    # Parent process
    print "b\n";
    waitpid($pid, 0);
} else {
    die "Cannot fork: $!\n";
}

