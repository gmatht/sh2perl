#!/usr/bin/env perl
use strict;
use warnings;
if (-f "file.txt") {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
print "exit: ${\($? >> 8)}\n";

