#!/usr/bin/perl

# Example 038: Advanced control flow using system() and backticks
# This demonstrates advanced control flow with builtins called from Perl

print "=== Example 038: Advanced control flow ===\n";

# Create test files
open(my $fh, '>', 'test_advanced.txt') or die "Cannot create test file: $!\n";
print $fh "Line 1: This is a test\n";
print $fh "Line 2: Another test line\n";
print $fh "Line 3: Third test line\n";
print $fh "Line 4: Fourth test line\n";
print $fh "Line 5: Fifth test line\n";
close($fh);

# Advanced if-else with builtins using system()
print "Advanced if-else with builtins:\n";

chomp $file_size;
if ($file_size > 100) {
    print "File is large ($file_size bytes), compressing:\n";
    system("gzip", "test_advanced.txt");
} elsif ($file_size > 50) {
    print "File is medium ($file_size bytes), processing:\n";
    system("cat", "test_advanced.txt", "|", "grep", "test");
} else {
    print "File is small ($file_size bytes), displaying:\n";
    system("cat", "test_advanced.txt");
}

# Nested loops with builtins using backticks
print "\nNested loops with builtins:\n";
for my $i (1..3) {
    print "Outer loop iteration $i:\n";
    for my $j (1..2) {
        print "  Inner loop iteration $j:\n";
        
        print "  $output";
    }
}

# Switch-like statement with builtins using system()
print "\nSwitch-like statement with builtins:\n";
my $command = "count";
if ($command eq "count") {
    print "Counting lines:\n";
    system("wc", "-l", "test_advanced.txt");
} elsif ($command eq "sort") {
    print "Sorting lines:\n";
    system("sort", "test_advanced.txt");
} elsif ($command eq "grep") {
    print "Grepping lines:\n";
    system("grep", "test", "test_advanced.txt");
} else {
    print "Unknown command, displaying file:\n";
    system("cat", "test_advanced.txt");
}

# Function with builtins using backticks
print "\nFunction with builtins:\n";
sub process_file_with_builtins {
    my ($filename, $operation) = @_;
    
    if ($operation eq "lines") {
        
        return $result;
    } elsif ($operation eq "words") {
        
        return $result;
    } elsif ($operation eq "chars") {
        
        return $result;
    } else {
        
        return $result;
    }
}

my $lines = process_file_with_builtins("test_advanced.txt", "lines");
print "Lines: $lines";

# Error handling with builtins using system()
print "\nError handling with builtins:\n";
my $result = system("grep", "nonexistent", "test_advanced.txt");
if ($result == 0) {
    print "Pattern found\n";
} else {
    print "Pattern not found, trying alternative:\n";
    system("grep", "test", "test_advanced.txt");
}

# Conditional execution with builtins using backticks
print "\nConditional execution with builtins:\n";

print $file_exists;

# Loop with break and continue using system()
print "\nLoop with break and continue:\n";
for my $i (1..5) {
    print "Processing iteration $i:\n";
    if ($i == 3) {
        print "Skipping iteration $i\n";
        next;
    }
    if ($i == 4) {
        print "Breaking at iteration $i\n";
        last;
    }
    system("echo", "Processed iteration $i");
}

# Recursive function with builtins using backticks
print "\nRecursive function with builtins:\n";
sub recursive_process {
    my ($depth, $max_depth) = @_;
    
    if ($depth > $max_depth) {
        return;
    }
    
    print "  " x $depth . "Depth $depth:\n";
    
    print "  " x $depth . $output;
    
    recursive_process($depth + 1, $max_depth);
}

recursive_process(1, 3);

# Exception handling with builtins using system()
print "\nException handling with builtins:\n";
eval {
    system("nonexistent_command", "test_advanced.txt");
    if ($? != 0) {
        die "Command failed with exit code $?";
    }
};
if ($@) {
    print "Exception caught: $@\n";
    print "Falling back to safe operation:\n";
    system("cat", "test_advanced.txt");
}

# Complex conditional with builtins using backticks
print "\nComplex conditional with builtins:\n";

chomp $file_size_check;

chomp $line_count;

if ($file_size_check > 50 && $line_count > 2) {
    print "File meets criteria (size: $file_size_check, lines: $line_count):\n";
    
    print $output;
} else {
    print "File does not meet criteria\n";
}

# Clean up
unlink('test_advanced.txt') if -f 'test_advanced.txt';
unlink('test_advanced.txt.gz') if -f 'test_advanced.txt.gz';

print "=== Example 038 completed successfully ===\n";
