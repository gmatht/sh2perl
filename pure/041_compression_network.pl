#!/usr/bin/perl

# Example 041: Compression and network operations using system() and backticks
# This demonstrates compression and network builtins called from Perl

print "=== Example 041: Compression and network operations ===\n";

# Create test file for compression
open(my $fh, '>', 'test_compress.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for compression\n";
print $fh "It contains multiple lines of text\n";
print $fh "To demonstrate compression functionality\n";
print $fh "With various content types\n";
print $fh "Including numbers: 12345\n";
print $fh "And special characters: !@#$%^&*()\n";
close($fh);

# gzip compression using system()
print "Using system() to call gzip (compress file):\n";
system("gzip", "test_compress.txt");
if (-f "test_compress.txt.gz") {
    print "File compressed successfully\n";
    
    print "Compressed size: $compressed_size bytes\n";
}

# zcat decompression using backticks
print "\nUsing backticks to call zcat (decompress and display):\n";

print $zcat_output;

# gzip with different compression levels using system()
print "\ngzip with different compression levels:\n";
system("gzip", "-1", "test_compress.txt.gz", "-c", ">", "test_compress_fast.gz");
system("gzip", "-9", "test_compress.txt.gz", "-c", ">", "test_compress_best.gz");

# Check compression ratios
if (-f "test_compress_fast.gz" && -f "test_compress_best.gz") {
    
    
    print "Fast compression size: $fast_size bytes\n";
    print "Best compression size: $best_size bytes\n";
}

# gzip with verbose output using backticks
print "\ngzip with verbose output:\n";

print $gzip_verbose;

# zcat with multiple files using system()
print "\nzcat with multiple files:\n";
system("zcat", "test_compress.txt.gz", "test_compress_best.gz");

# gzip with keep original using backticks
print "\ngzip with keep original (-k):\n";

print $gzip_keep;

# gzip with test using system()
print "\ngzip with test (-t):\n";
system("gzip", "-t", "test_compress.txt.gz");

# zcat with pipe using backticks
print "\nzcat with pipe:\n";

print $zcat_pipe;

# gzip with force using system()
print "\ngzip with force (-f):\n";
system("gzip", "-f", "test_compress_best.gz");

# zcat with error handling using backticks
print "\nzcat with error handling:\n";

print "Error result: $zcat_error";

# gzip with recursive using system()
print "\ngzip with recursive (-r):\n";
system("mkdir", "-p", "test_compress_dir");
system("cp", "test_compress.txt.gz", "test_compress_dir/");
system("gzip", "-r", "test_compress_dir");

# zcat with output redirection using backticks
print "\nzcat with output redirection:\n";

if (-f "test_decompressed.txt") {
    print "File decompressed successfully\n";
    
    print "Decompressed content:\n$decompressed_content";
}

# Clean up
unlink('test_compress.txt.gz') if -f 'test_compress.txt.gz';
unlink('test_compress_fast.gz') if -f 'test_compress_fast.gz';
unlink('test_compress_best.gz') if -f 'test_compress_best.gz';
unlink('test_decompressed.txt') if -f 'test_decompressed.txt';
system("rm", "-rf", "test_compress_dir");

print "=== Example 041 completed successfully ===\n";
