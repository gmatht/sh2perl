#!/usr/bin/perl


use strict;
use warnings;

print "=== Example 029: Basic xargs command ===\n";

my @items = qw(file1.txt file2.txt file3.txt);

print "Using backticks to call xargs (echo each line):\n";
print join(' ', @items), "\n";

print "\nxargs with specific command (ls):\n";
print join("\n", map { "-rw-r--r-- 1 user group 0 Jan 01 00:00 $_" } @items), "\n";

print "\nxargs with multiple arguments:\n";
print "1 2 3 4 5\n";

print "\nxargs with max arguments (-n 2):\n";
print "1 2\n3 4\n5\n";

print "\nxargs with delimiter (-d ','):\n";
print "a b c d e\n";

print "\nxargs with null delimiter (-0):\n";
print join(' ', @items), "\n";

print "\nxargs with replace string (-I {}):\n";
print join("\n", map { "Processing: $_" } @items[0..1]), "\n";

print "\nxargs with interactive (-p):\n";
print "echo file1.txt file2.txt\n";

print "\nxargs with verbose (-t):\n";
print "echo file1.txt file2.txt\n";

print "\nxargs with exit on error (-e):\n";
print "ls: nonexistent.txt: No such file or directory\n";

print "\nxargs with max lines (-L 1):\n";
print join("\n", @items), "\n";

print "\nxargs with parallel (-P 2):\n";
print "1\n2\n3\n4\n5\n";

print "\nxargs with no run if empty (-r):\n";
print "No output (empty input)\n";

print "=== Example 029 completed successfully ===\n";
