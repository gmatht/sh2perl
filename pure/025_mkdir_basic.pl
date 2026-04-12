#!/usr/bin/perl

# Example 025: Basic mkdir command using system() and backticks
# This demonstrates the mkdir builtin called from Perl

print "=== Example 025: Basic mkdir command ===\n";

# Simple mkdir using system()
print "Using system() to call mkdir (create directory):\n";
system("mkdir", "test_mkdir_dir");
if (-d "test_mkdir_dir") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed\n";
}

# mkdir with parents using system()
print "\nmkdir with parents (-p):\n";
system("mkdir", "-p", "test_mkdir_parents/subdir1/subdir2");
if (-d "test_mkdir_parents/subdir1/subdir2") {
    print "Nested directories created successfully\n";
}

# mkdir with mode using backticks
print "\nmkdir with mode (-m 755):\n";

if (-d "test_mkdir_mode") {
    print "Directory created with mode 755\n";
}

# mkdir with verbose using system()
print "\nmkdir with verbose (-v):\n";
system("mkdir", "-v", "test_mkdir_verbose");

# mkdir with multiple directories using backticks
print "\nmkdir with multiple directories:\n";

if (-d "test_mkdir_multi1" && -d "test_mkdir_multi2" && -d "test_mkdir_multi3") {
    print "Multiple directories created successfully\n";
}

# mkdir with parents and mode using system()
print "\nmkdir with parents and mode (-p -m 700):\n";
system("mkdir", "-p", "-m", "700", "test_mkdir_secure/subdir");
if (-d "test_mkdir_secure/subdir") {
    print "Secure directory created successfully\n";
}

# mkdir with parents and verbose using backticks
print "\nmkdir with parents and verbose (-p -v):\n";

print $mkdir_pv;

# mkdir with ignore existing using system()
print "\nmkdir with ignore existing (-p):\n";
system("mkdir", "-p", "test_mkdir_dir");  # Try to create existing directory
print "Attempted to create existing directory (should not fail with -p)\n";

# mkdir with specific mode using backticks
print "\nmkdir with specific mode (777):\n";

if (-d "test_mkdir_777") {
    print "Directory created with mode 777\n";
}

# mkdir with parents and multiple directories using system()
print "\nmkdir with parents and multiple directories:\n";
system("mkdir", "-p", "test_mkdir_batch1/subdir", "test_mkdir_batch2/subdir", "test_mkdir_batch3/subdir");
if (-d "test_mkdir_batch1/subdir" && -d "test_mkdir_batch2/subdir" && -d "test_mkdir_batch3/subdir") {
    print "Batch directories created successfully\n";
}

# mkdir with error handling using backticks
print "\nmkdir with error handling:\n";

if (-d "test_mkdir_error") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed or already exists\n";
}

# Clean up
system("rm", "-rf", "test_mkdir_dir");
system("rm", "-rf", "test_mkdir_parents");
system("rm", "-rf", "test_mkdir_mode");
system("rm", "-rf", "test_mkdir_verbose");
system("rm", "-rf", "test_mkdir_multi1");
system("rm", "-rf", "test_mkdir_multi2");
system("rm", "-rf", "test_mkdir_multi3");
system("rm", "-rf", "test_mkdir_secure");
system("rm", "-rf", "test_mkdir_pv");
system("rm", "-rf", "test_mkdir_777");
system("rm", "-rf", "test_mkdir_batch1");
system("rm", "-rf", "test_mkdir_batch2");
system("rm", "-rf", "test_mkdir_batch3");
system("rm", "-rf", "test_mkdir_error");

print "=== Example 025 completed successfully ===\n";
