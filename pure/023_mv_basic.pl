#!/usr/bin/perl

# Example 023: Basic mv command using system() and backticks
# This demonstrates the mv builtin called from Perl

print "=== Example 023: Basic mv command ===\n";

# Create test files first
open(my $fh, '>', 'test_mv_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for moving\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate mv functionality\n";
close($fh);

# Create test directory
system("mkdir", "-p", "test_mv_dir");

# Simple mv using system()
print "Using system() to call mv (move file):\n";
system("mv", "test_mv_source.txt", "test_mv_dest.txt");
if (-f "test_mv_dest.txt") {
    print "File moved successfully\n";
    
    print "Content: $content";
} else {
    print "File move failed\n";
}

# mv with verbose using system()
print "\nmv with verbose (-v):\n";
system("mv", "-v", "test_mv_dest.txt", "test_mv_verbose.txt");

# mv with force using backticks
print "\nmv with force (-f):\n";

if (-f "test_mv_force.txt") {
    print "File moved with force\n";
}

# mv with interactive using system()
print "\nmv with interactive (-i):\n";
system("mv", "-i", "test_mv_force.txt", "test_mv_interactive.txt");

# mv with backup using backticks
print "\nmv with backup (-b):\n";

if (-f "test_mv_backup.txt") {
    print "File moved with backup\n";
}

# mv with suffix using system()
print "\nmv with suffix (--suffix=.bak):\n";
system("mv", "--suffix=.bak", "test_mv_backup.txt", "test_mv_suffix.txt");

# mv with no target directory using backticks
print "\nmv with no target directory (-T):\n";

if (-f "test_mv_no_target.txt") {
    print "File moved with no target directory\n";
}

# mv with update using system()
print "\nmv with update (-u):\n";
system("mv", "-u", "test_mv_no_target.txt", "test_mv_update.txt");

# mv with no clobber using backticks
print "\nmv with no clobber (-n):\n";

if (-f "test_mv_no_clobber.txt") {
    print "File moved with no clobber\n";
}

# mv with strip trailing slashes using system()
print "\nmv with strip trailing slashes (--strip-trailing-slashes):\n";
system("mv", "--strip-trailing-slashes", "test_mv_no_clobber.txt", "test_mv_strip.txt");

# mv with multiple files using backticks
print "\nmv with multiple files:\n";
# Create multiple files first
system("touch", "test_mv_file1.txt");
system("touch", "test_mv_file2.txt");

if (-f "test_mv_dir/test_mv_file1.txt" && -f "test_mv_dir/test_mv_file2.txt") {
    print "Multiple files moved successfully\n";
}

# mv with preserve all using system()
print "\nmv with preserve all (-a):\n";
system("touch", "test_mv_preserve.txt");
system("mv", "-a", "test_mv_preserve.txt", "test_mv_preserve_dest.txt");

# Clean up
unlink('test_mv_strip.txt') if -f 'test_mv_strip.txt';
unlink('test_mv_preserve_dest.txt') if -f 'test_mv_preserve_dest.txt';
system("rm", "-rf", "test_mv_dir");

print "=== Example 023 completed successfully ===\n";
