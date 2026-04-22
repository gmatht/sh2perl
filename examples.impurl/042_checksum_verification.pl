#!/usr/bin/perl

# Example 042: Checksum verification using system() and backticks
# This demonstrates checksum builtins called from Perl

print "=== Example 042: Checksum verification ===\n";

# Create test file for checksums
open(my $fh, '>', 'test_checksum.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for checksum verification\n";
print $fh "It contains data that will be hashed\n";
print $fh "To demonstrate checksum functionality\n";
print $fh "With various content types\n";
print $fh "Including numbers: 12345\n";
print $fh "And special characters: !@#$%^&*()\n";
close($fh);

# SHA256 checksum using system()
print "Using system() to call sha256sum:\n";
system("sha256sum", "test_checksum.txt");

# SHA256 checksum using backticks
print "\nUsing backticks to call sha256sum:\n";
my $sha256_output = `sha256sum test_checksum.txt`;
print $sha256_output;

# SHA512 checksum using system()
print "\nUsing system() to call sha512sum:\n";
system("sha512sum", "test_checksum.txt");

# SHA512 checksum using backticks
print "\nUsing backticks to call sha512sum:\n";
my $sha512_output = `sha512sum test_checksum.txt`;
print $sha512_output;

# SHA256 with check using system()
print "\nSHA256 with check (-c):\n";
# create checksum file via shell redirection
system("sh", "-c", "sha256sum test_checksum.txt > test_checksum.sha256");
system("sha256sum", "-c", "test_checksum.sha256");

# SHA512 with check using backticks
print "\nSHA512 with check (-c):\n";
my $sha512_check = `sha512sum test_checksum.txt > test_checksum.sha512 && sha512sum -c test_checksum.sha512`;
print $sha512_check;

# SHA256 with binary mode using system()
print "\nSHA256 with binary mode (-b):\n";
system("sha256sum", "-b", "test_checksum.txt");

# SHA512 with text mode using backticks
print "\nSHA512 with text mode (-t):\n";
my $sha512_text = `sha512sum -t test_checksum.txt`;
print $sha512_text;

# SHA256 with status using system()
print "\nSHA256 with status (--status):\n";
system("sha256sum", "--status", "-c", "test_checksum.sha256");

# SHA512 with warn using backticks
print "\nSHA512 with warn (--warn):\n";
my $sha512_warn = `sha512sum --warn -c test_checksum.sha512`;
print $sha512_warn;

# SHA256 with quiet using system()
print "\nSHA256 with quiet (--quiet):\n";
system("sha256sum", "--quiet", "-c", "test_checksum.sha256");

# SHA512 with strict using backticks
print "\nSHA512 with strict (--strict):\n";
my $sha512_strict = `sha512sum --strict -c test_checksum.sha512`;
print $sha512_strict;

# SHA256 with ignore missing using system()
print "\nSHA256 with ignore missing (--ignore-missing):\n";
system("sha256sum", "--ignore-missing", "-c", "test_checksum.sha256");

# SHA512 with ignore missing using backticks
print "\nSHA512 with ignore missing (--ignore-missing):\n";
my $sha512_ignore = `sha512sum --ignore-missing -c test_checksum.sha512`;
print $sha512_ignore;

# SHA256 with multiple files using system()
print "\nSHA256 with multiple files:\n";
system("cp", "test_checksum.txt", "test_checksum_copy.txt");
system("sha256sum", "test_checksum.txt", "test_checksum_copy.txt");

# SHA512 with multiple files using backticks
print "\nSHA512 with multiple files:\n";
my $sha512_multi = `sha512sum test_checksum.txt test_checksum_copy.txt`;
print $sha512_multi;

# SHA256 with pipe using system()
print "\nSHA256 with pipe:\n";
system("sh", "-c", "cat test_checksum.txt | sha256sum");

# SHA512 with pipe using backticks
print "\nSHA512 with pipe:\n";
my $sha512_pipe = `cat test_checksum.txt | sha512sum`;
print $sha512_pipe;

# Clean up
unlink('test_checksum.txt') if -f 'test_checksum.txt';
unlink('test_checksum_copy.txt') if -f 'test_checksum_copy.txt';
unlink('test_checksum.sha256') if -f 'test_checksum.sha256';
unlink('test_checksum.sha512') if -f 'test_checksum.sha512';

print "=== Example 042 completed successfully ===\n";
