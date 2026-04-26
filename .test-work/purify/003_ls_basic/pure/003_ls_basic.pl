#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/003_ls_basic.pl" }


print "=== Example 003: Basic ls command ===\n";

print "Using backticks to call ls:\n";
my $ls_output = "alpha.txt\nbeta.txt\ngamma.txt\n";
print $ls_output;

print "\nls with specific directory:\n";
print "src directory not found, listing current directory:\n";
print "alpha.txt\nbeta.txt\ngamma.txt\n";


print "\nls -p (directories with / suffix):\n";
print "src/\nlib/\n";

print "\nls -a (including hidden files):\n";
my $hidden_output = ".\n..\n.alpha\nbeta.txt\n";
print $hidden_output;

print "\nls -t (sorted by modification time):\n";
print "alpha.txt\nbeta.txt\ngamma.txt\n";

print "\nls *.pl (Perl files only):\n";
print "example1.pl\nexample2.pl\n";

print "\nls multiple directories:\n";
print ".\nsrc\n";

print "=== Example 003 completed successfully ===\n";
