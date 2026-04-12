#!/usr/bin/perl

# Example 043: String processing using system() and backticks
# This demonstrates string processing builtins called from Perl

print "=== Example 043: String processing ===\n";

# Create test file for string processing
open(my $fh, '>', 'test_strings.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for string processing\n";
print $fh "It contains various types of content\n";
print $fh "Including numbers: 12345\n";
print $fh "And special characters: !@#$%^&*()\n";
print $fh "With mixed case: Hello World\n";
print $fh "And repeated words: test test test\n";
print $fh "Binary data: \x00\x01\x02\x03\n";
close($fh);

# strings command using system()
print "Using system() to call strings:\n";
system("strings", "test_strings.txt");

# strings command using backticks
print "\nUsing backticks to call strings:\n";

print $strings_output;

# strings with minimum length using system()
print "\nstrings with minimum length (-n 5):\n";
system("strings", "-n", "5", "test_strings.txt");

# strings with minimum length using backticks
print "\nstrings with minimum length (-n 10):\n";

print $strings_min;

# strings with encoding using system()
print "\nstrings with encoding (-e s):\n";
system("strings", "-e", "s", "test_strings.txt");

# strings with encoding using backticks
print "\nstrings with encoding (-e l):\n";

print $strings_enc;

# strings with all encodings using system()
print "\nstrings with all encodings (-a):\n";
system("strings", "-a", "test_strings.txt");

# strings with null separator using backticks
print "\nstrings with null separator (-z):\n";

print $strings_null;

# strings with print file name using system()
print "\nstrings with print file name (-f):\n";
system("strings", "-f", "test_strings.txt");

# strings with print file name using backticks
print "\nstrings with print file name (-f):\n";

print $strings_file;

# strings with print file name and null separator using system()
print "\nstrings with print file name and null separator (-f -z):\n";
system("strings", "-f", "-z", "test_strings.txt");

# strings with print file name and null separator using backticks
print "\nstrings with print file name and null separator (-f -z):\n";

print $strings_file_null;

# strings with multiple files using system()
print "\nstrings with multiple files:\n";
system("cp", "test_strings.txt", "test_strings_copy.txt");
system("strings", "test_strings.txt", "test_strings_copy.txt");

# strings with multiple files using backticks
print "\nstrings with multiple files:\n";

print $strings_multi;

# strings with pipe using system()
print "\nstrings with pipe:\n";
system("cat", "test_strings.txt", "|", "strings");

# strings with pipe using backticks
print "\nstrings with pipe:\n";

print $strings_pipe;

# strings with error handling using system()
print "\nstrings with error handling:\n";
system("strings", "nonexistent_file.txt", "2>/dev/null", "||", "echo", "File not found");

# strings with error handling using backticks
print "\nstrings with error handling:\n";

print "Error result: $strings_error";

# Clean up
unlink('test_strings.txt') if -f 'test_strings.txt';
unlink('test_strings_copy.txt') if -f 'test_strings_copy.txt';

print "=== Example 043 completed successfully ===\n";
