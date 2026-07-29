#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = '030_control_flow_if.sh';
if (-f "file.txt") {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
print "exit: ${\($? >> 8)}\n";
