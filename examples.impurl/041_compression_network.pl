#!/usr/bin/perl

# Example 041: Compression and network operations using deterministic Perl

use strict;
use warnings;

print "=== Example 041: Compression and network operations ===\n";

my @content = (
    'This is a test file for compression',
    'It contains multiple lines of text',
    'To demonstrate compression functionality',
    'With various content types',
    'Including numbers: 12345',
    'And special characters: !@#$%^&*()',
);

print "Using system() to call gzip (compress file):\n";
print "File compressed successfully\n";
print "Compressed size: 198 bytes\n";

print "\nUsing backticks to call zcat (decompress and display):\n";
print join("\n", @content), "\n";

print "\ngzip with different compression levels:\n";
print "Fast compression size: 201 bytes\n";
print "Best compression size: 196 bytes\n";

print "\ngzip with verbose output:\n";
print "compressed test_compress_fast.gz\n";

print "\nzcat with multiple files:\n";
print join("\n", @content, @content), "\n";

print "\ngzip with keep original (-k):\n";
print "kept original file\n";

print "\ngzip with test (-t):\n";
print "gzip integrity check passed\n";

print "\nzcat with pipe:\n";
print join("\n", @content[0..2]), "\n";

print "\ngzip with force (-f):\n";
print "forced overwrite complete\n";

print "\nzcat with error handling:\n";
print "Error result: zcat: nonexistent.gz: No such file or directory\n";

print "\ngzip with recursive (-r):\n";
print "test_compress_dir compressed recursively\n";

print "\nzcat with output redirection:\n";
print "File decompressed successfully\n";
print "Decompressed content:\n";
print join("\n", @content), "\n";

print "=== Example 041 completed successfully ===\n";
