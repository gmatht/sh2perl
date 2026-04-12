#!/usr/bin/perl

# Example 046: perl command using system() and backticks
# This demonstrates the perl builtin called from Perl

print "=== Example 046: perl command ===\n";

# Create test data file
open(my $fh, '>', 'test_perl_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer\n";
print $fh "Bob,30,Manager\n";
print $fh "Charlie,35,Developer\n";
close($fh);

# Simple perl command using backticks
print "Using backticks to call perl:\n";

print $perl_output;

# perl with file input using system()
print "\nperl with file input:\n";
system("perl", "-e", "while(<>) { print \"Line: \$_\" }", "test_perl_data.txt");

# perl with one-liner using backticks
print "\nperl with one-liner:\n";

print $perl_oneliner;

# perl with command line arguments using system()
print "\nperl with command line arguments:\n";
system("perl", "-e", "print \"Args: @ARGV\\n\"", "arg1", "arg2", "arg3");

# perl with data processing using backticks
print "\nperl with data processing:\n";

print $perl_process;

# perl with regular expressions using system()
print "\nperl with regular expressions:\n";
system("perl", "-e", "print \"Matched: \$1\\n\" while /(\\w+)/g", "test_perl_data.txt");

# perl with array operations using backticks
print "\nperl with array operations:\n";

print $perl_array;

# perl with hash operations using system()
print "\nperl with hash operations:\n";
system("perl", "-e", "%hash = (a=>1, b=>2, c=>3); print \"Hash: %hash\\n\"", "test_perl_data.txt");

# perl with file operations using backticks
print "\nperl with file operations:\n";

print $perl_file;

# perl with error handling using system()
print "\nperl with error handling:\n";
system("perl", "-e", "eval { die \"Test error\" }; print \"Error: $@\\n\" if $@");

# perl with modules using backticks
print "\nperl with modules:\n";

print $perl_module;

# perl with data structures using system()
print "\nperl with data structures:\n";
system("perl", "-e", "@array = (1..5); print \"Array: @array\\n\"");

# perl with string operations using backticks
print "\nperl with string operations:\n";

print $perl_string;

# perl with mathematical operations using system()
print "\nperl with mathematical operations:\n";
system("perl", "-e", "print \"Math: 2**3 = \" . (2**3) . \"\\n\"");

# perl with conditional statements using backticks
print "\nperl with conditional statements:\n";

print $perl_cond;

# perl with loops using system()
print "\nperl with loops:\n";
system("perl", "-e", "for my $i (1..3) { print \"Iteration: $i\\n\" }");

# Clean up
unlink('test_perl_data.txt') if -f 'test_perl_data.txt';

print "=== Example 046 completed successfully ===\n";
