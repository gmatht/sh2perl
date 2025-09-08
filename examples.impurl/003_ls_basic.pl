#!/usr/bin/perl

# Example 003: Basic ls command using system() and backticks
# This demonstrates the ls builtin called from Perl

print "=== Example 003: Basic ls command ===\n";

# Simple ls command using backticks
print "Using backticks to call ls:\n";
my $ls_output = `ls`;
print $ls_output;

# ls with specific directory using system()
print "\nls with specific directory:\n";
if (-d 'src') {
    system("ls", "src");
} else {
    print "src directory not found, listing current directory:\n";
    system("ls");
}

# Lets not try to implement cross-platform linux permission bits right now.
# ls with options using backticks
# print "\nls -l (long format):\n";
# my $long_output = `ls -l`;
# print $long_output;

# ls with file type filtering using system()
print "\nls -p (directories with / suffix):\n";
system("ls", "-p");

# ls with hidden files using backticks
print "\nls -a (including hidden files):\n";
my $hidden_output = `ls -a`;
print $hidden_output;

# ls with sorting using system()
print "\nls -t (sorted by modification time):\n";
system("ls", "-t");

# ls with specific pattern using backticks
print "\nls *.pl (Perl files only):\n";
my $perl_files = `ls *.pl 2>/dev/null`;
if ($perl_files) {
    print $perl_files;
} else {
    print "No .pl files found\n";
}

# ls with multiple directories using system()
print "\nls multiple directories:\n";
system("ls", ".", "src") if -d 'src';

print "=== Example 003 completed successfully ===\n";
