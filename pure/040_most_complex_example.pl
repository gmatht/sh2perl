#!/usr/bin/perl

# Example 040: Most complex example using system() and backticks
# This demonstrates the most complex operations with builtins called from Perl

print "=== Example 040: Most complex example ===\n";

# Create complex test data
open(my $fh, '>', 'complex_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5,New York\n";
print $fh "Bob,30,Manager,87.2,Los Angeles\n";
print $fh "Charlie,35,Developer,92.8,Chicago\n";
print $fh "Diana,28,Designer,88.9,San Francisco\n";
print $fh "Eve,32,Analyst,91.3,Boston\n";
print $fh "Frank,29,Engineer,89.7,Seattle\n";
print $fh "Grace,31,Manager,93.1,Austin\n";
print $fh "Henry,27,Developer,86.4,Denver\n";
print $fh "Ivy,33,Designer,94.2,Portland\n";
print $fh "Jack,26,Analyst,85.8,Miami\n";
close($fh);

# Complex data processing pipeline
print "Complex data processing pipeline:\n";
print "Processing employee data with multiple filters and transformations...\n";

# Step 1: Filter and transform data
my $step1 = `cat complex_data.txt | grep -E 'Engineer|Developer' | cut -d',' -f1,2,4,5 | tr ',' '|' | sort -t'|' -k3 -nr`;
print "Step 1 - Filtered and transformed data:\n$step1";

# Step 2: Advanced analysis with multiple commands
print "\nStep 2 - Advanced analysis:\n";

print "Top roles by count:\n$step2";

# Step 3: Complex conditional processing
print "\nStep 3 - Complex conditional processing:\n";

chomp $file_size;
if ($file_size > 200) {
    print "Large file detected ($file_size bytes), performing compression:\n";
    system("gzip", "complex_data.txt");
    print "File compressed\n";
} else {
    print "File size acceptable ($file_size bytes), proceeding with analysis:\n";
}

# Step 4: Multi-step data aggregation
print "\nStep 4 - Multi-step data aggregation:\n";

print "Average scores by role:\n$step4";

# Step 5: Complex file operations with error handling
print "\nStep 5 - Complex file operations:\n";
system("mkdir", "-p", "temp_analysis");
system("cat", "complex_data.txt", "|", "grep", "Engineer", ">", "temp_analysis/engineers.txt");
system("cat", "complex_data.txt", "|", "grep", "Manager", ">", "temp_analysis/managers.txt");
system("cat", "complex_data.txt", "|", "grep", "Developer", ">", "temp_analysis/developers.txt");

# Check if files were created
if (-f "temp_analysis/engineers.txt") {
    
    print "Engineers file created with $engineers lines\n";
}

# Step 6: Advanced pipeline with multiple filters
print "\nStep 6 - Advanced pipeline:\n";

print "Top performers across all roles:\n$step6";

# Step 7: Complex data validation and reporting
print "\nStep 7 - Data validation and reporting:\n";

chomp $validation;
if ($validation > 0) {
    print "Data validation failed: $validation invalid lines found\n";
} else {
    print "Data validation passed: All lines have correct format\n";
}

# Step 8: Statistical analysis with builtins
print "\nStep 8 - Statistical analysis:\n";

print "Score statistics:\n$stats";

# Step 9: Complex error handling and recovery
print "\nStep 9 - Error handling and recovery:\n";
eval {
    my $result = system("grep", "nonexistent_pattern", "complex_data.txt");
    if ($result != 0) {
        print "Pattern not found, trying alternative approach:\n";
        
        print "Alternative count: $alt_result";
    }
};

# Step 10: Final cleanup and summary
print "\nStep 10 - Final cleanup and summary:\n";




print "Summary:\n";
print "Total lines: $total_lines";
print "Total characters: $total_chars";
print "Unique roles: $unique_roles";

# Clean up
unlink('complex_data.txt') if -f 'complex_data.txt';
unlink('complex_data.txt.gz') if -f 'complex_data.txt.gz';
system("rm", "-rf", "temp_analysis");

print "=== Example 040 completed successfully ===\n";
