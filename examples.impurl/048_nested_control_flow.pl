#!/usr/bin/perl

# Example 048: Nested control flow using system() and backticks
# This demonstrates nested control flow with builtins called from Perl

print "=== Example 048: Nested control flow ===\n";

# Create test files
open(my $fh, '>', 'nested_test.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
print $fh "This is line four\n";
print $fh "This is line five\n";
close($fh);

# Nested if-else with builtins using system()
print "Nested if-else with builtins:\n";
my $file_size = `wc -c nested_test.txt | cut -d' ' -f1`;
chomp $file_size;
if ($file_size > 50) {
    print "File is large ($file_size bytes), checking content:\n";
    my $line_count = `wc -l nested_test.txt | cut -d' ' -f1`;
    chomp $line_count;
    if ($line_count > 3) {
        print "File has many lines ($line_count), processing:\n";
        system("sh", "-c", "cat nested_test.txt | head -3");
    } else {
        print "File has few lines ($line_count), displaying all:\n";
        system("cat", "nested_test.txt");
    }
} else {
    print "File is small ($file_size bytes), displaying:\n";
    system("cat", "nested_test.txt");
}

# Nested loops with builtins using backticks
print "\nNested loops with builtins:\n";
for my $i (1..3) {
    print "Outer loop iteration $i:\n";
    for my $j (1..2) {
        print "  Inner loop iteration $j:\n";
        my $output = `echo "Nested: $i-$j"`;
        print "  $output";
        
        # Inner condition
        if ($j == 2) {
            print "  Inner condition met, processing:\n";
            my $process_output = `echo "Processing $i-$j" | tr 'a-z' 'A-Z'`;
            print "  $process_output";
        }
    }
}

# Nested functions with builtins using system()
print "\nNested functions with builtins:\n";
sub outer_function {
    my ($param) = @_;
    print "Outer function called with: $param\n";
    
    sub inner_function {
        my ($inner_param) = @_;
        print "  Inner function called with: $inner_param\n";
        system("echo", "Inner processing: $inner_param");
    }
    
    inner_function("inner_value");
    system("echo", "Outer processing: $param");
}

outer_function("outer_value");

# Nested error handling with builtins using backticks
print "\nNested error handling with builtins:\n";
eval {
    my $result = system("grep", "nonexistent", "nested_test.txt");
    if ($result != 0) {
        print "Pattern not found, trying alternative:\n";
        eval {
            my $alt_result = `grep 'line' nested_test.txt`;
            if ($alt_result) {
                print "Alternative pattern found:\n$alt_result";
            } else {
                print "No patterns found, displaying file:\n";
                system("cat", "nested_test.txt");
            }
        };
        if ($@) {
            print "Alternative also failed: $@\n";
        }
    }
};

# Nested conditional execution with builtins using system()
print "\nNested conditional execution with builtins:\n";
my $file_exists = `test -f nested_test.txt && echo "File exists" || echo "File not found"`;
if ($file_exists =~ /exists/) {
    print "File exists, checking permissions:\n";
    my $permissions = `ls -l nested_test.txt | cut -d' ' -f1`;
    print "Permissions: $permissions";
    
    if ($permissions =~ /r/) {
        print "File is readable, processing:\n";
        system("sh", "-c", "cat nested_test.txt | head -2");
    } else {
        print "File is not readable\n";
    }
} else {
    print "File does not exist\n";
}

# Nested data processing with builtins using backticks
print "\nNested data processing with builtins:\n";
my $data = `cat nested_test.txt`;
my @lines = split(/\n/, $data);
for my $i (0..$#lines) {
    print "Processing line $i:\n";
    if ($lines[$i] =~ /line/) {
        print "  Line contains 'line', processing:\n";
        my $processed = `echo "$lines[$i]" | tr 'a-z' 'A-Z'`;
        print "  $processed";
        
        if ($i > 1) {
            print "  Line number > 1, additional processing:\n";
            my $additional = `echo "$lines[$i]" | wc -c`;
            print "  Character count: $additional";
        }
    } else {
        print "  Line does not contain 'line'\n";
    }
}

# Nested file operations with builtins using system()
print "\nNested file operations with builtins:\n";
if (-f "nested_test.txt") {
    print "File exists, creating backup:\n";
    system("cp", "nested_test.txt", "nested_test_backup.txt");
    
    if (-f "nested_test_backup.txt") {
        print "Backup created, processing original:\n";
        system("sh", "-c", "cat nested_test.txt | grep line | wc -l");
        
        print "Cleaning up backup:\n";
        system("rm", "nested_test_backup.txt");
    } else {
        print "Backup creation failed\n";
    }
} else {
    print "File does not exist\n";
}

# Clean up
unlink('nested_test.txt') if -f 'nested_test.txt';
unlink('nested_test_backup.txt') if -f 'nested_test_backup.txt';

print "=== Example 048 completed successfully ===\n";
