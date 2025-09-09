#!/usr/bin/env perl

use strict;
use warnings;
use File::Path qw(make_path remove_tree);
use File::Spec;

# Create test data for benchmarking
# This script creates a consistent test environment for fair comparisons

my $TEST_DIR = "benchmark_test_data";
my $NUM_FILES = 100;
my $NUM_DIRS = 10;

sub create_test_environment {
    print "Creating test environment: $TEST_DIR\n";
    
    # Remove existing test directory
    if (-d $TEST_DIR) {
        remove_tree($TEST_DIR);
    }
    
    # Create main test directory
    make_path($TEST_DIR);
    
    # Create subdirectories
    for my $i (1..$NUM_DIRS) {
        my $subdir = File::Spec->catdir($TEST_DIR, "subdir_$i");
        make_path($subdir);
        
        # Create files in subdirectories
        for my $j (1..($NUM_FILES / $NUM_DIRS)) {
            my $file = File::Spec->catfile($subdir, "file_$j.txt");
            open(my $fh, '>', $file) or die "Cannot create $file: $!";
            print $fh "This is test file $j in subdirectory $i\n";
            print $fh "Line 2: Random data " . rand(1000) . "\n";
            print $fh "Line 3: More test content\n";
            close($fh);
        }
    }
    
    # Create some empty files
    for my $i (1..5) {
        my $empty_file = File::Spec->catfile($TEST_DIR, "empty_$i.txt");
        open(my $fh, '>', $empty_file) or die "Cannot create $empty_file: $!";
        close($fh);
    }
    
    # Create some large files (>1MB)
    for my $i (1..3) {
        my $large_file = File::Spec->catfile($TEST_DIR, "large_$i.txt");
        open(my $fh, '>', $large_file) or die "Cannot create $large_file: $!";
        # Write 1MB of data
        for my $line (1..10000) {
            print $fh "This is line $line of large file $i. " . ("x" x 100) . "\n";
        }
        close($fh);
    }
    
    # Create some recently modified files
    for my $i (1..10) {
        my $recent_file = File::Spec->catfile($TEST_DIR, "recent_$i.txt");
        open(my $fh, '>', $recent_file) or die "Cannot create $recent_file: $!";
        print $fh "Recently modified file $i\n";
        close($fh);
        
        # Touch the file to make it recent
        utime(time(), time(), $recent_file);
    }
    
    # Create some old files
    for my $i (1..5) {
        my $old_file = File::Spec->catfile($TEST_DIR, "old_$i.txt");
        open(my $fh, '>', $old_file) or die "Cannot create $old_file: $!";
        print $fh "Old file $i\n";
        close($fh);
        
        # Make file old (8 days ago)
        my $old_time = time() - (8 * 24 * 60 * 60);
        utime($old_time, $old_time, $old_file);
    }
    
    print "Test environment created successfully!\n";
    print "Created:\n";
    print "  - $NUM_DIRS subdirectories\n";
    print "  - $NUM_FILES regular files\n";
    print "  - 5 empty files\n";
    print "  - 3 large files (>1MB)\n";
    print "  - 10 recently modified files\n";
    print "  - 5 old files (8+ days)\n";
}

sub cleanup_test_environment {
    print "Cleaning up test environment: $TEST_DIR\n";
    if (-d $TEST_DIR) {
        remove_tree($TEST_DIR);
        print "Test environment cleaned up.\n";
    }
}

# Main execution
if (@ARGV && $ARGV[0] eq "cleanup") {
    cleanup_test_environment();
} else {
    create_test_environment();
}

1;

