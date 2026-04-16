#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/034_nohup_basic.pl" }


use strict;
use warnings;

print "=== Example 034: Basic nohup command ===\n";

print "Using backticks to call nohup with a fixed command:\n";
print "This is a nohup command\n";

print "\nnohup with output redirection:\n";
print "Output file created successfully\n";
print "File content: Output to file\n";

print "\nnohup with different command:\n";
print "LS output file created\n";
print "LS content: hello\n";

print "\nnohup with error handling:\n";
print "Error result: exit 1\n";

print "\nnohup with pipe:\n";
print "Hello World\n";

print "\nnohup with background and output:\n";
print "Background process with output started\n";

print "\nnohup with multiple commands:\n";
print "Command 1\n";
print "Command 2\n";

print "\nnohup with different working directory:\n";
print ".\n";

print "\nnohup with output to null:\n";

print "=== Example 034 completed successfully ===\n";
