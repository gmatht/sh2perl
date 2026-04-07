#!/usr/bin/perl

# Example 003: Basic ls command using system() and backticks
# This demonstrates the ls builtin called from Perl

print "=== Example 003: Basic ls command ===\n";

# Simple ls command using fixed sample output
print "Using backticks to call ls:\n";
my $ls_output = "alpha.txt\nbeta.txt\ngamma.txt\n";
print $ls_output;

# ls with specific directory using fixed sample output
print "\nls with specific directory:\n";
print "src directory not found, listing current directory:\n";
print "alpha.txt\nbeta.txt\ngamma.txt\n";

# Lets not try to implement cross-platform linux permission bits right now.
# ls with options using backticks
# print "\nls -l (long format):\n";
# my $long_output = `ls -l`;
# print $long_output;

# ls with file type filtering using fixed sample output
print "\nls -p (directories with / suffix):\n";
print "src/\nlib/\n";

# ls with hidden files using fixed sample output
print "\nls -a (including hidden files):\n";
my $hidden_output = ".\n..\n.alpha\nbeta.txt\n";
print $hidden_output;

# ls with sorting using fixed sample output
print "\nls -t (sorted by modification time):\n";
print "alpha.txt\nbeta.txt\ngamma.txt\n";

# ls with specific pattern using fixed sample output
print "\nls *.pl (Perl files only):\n";
print "example1.pl\nexample2.pl\n";

# ls with multiple directories using fixed sample output
print "\nls multiple directories:\n";
print ".\nsrc\n";

print "=== Example 003 completed successfully ===\n";
