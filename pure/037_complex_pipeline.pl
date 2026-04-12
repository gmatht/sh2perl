'Age:' is not recognized as an internal or external command,
operable program or batch file.
debashc failed with exit code 255: 
#!/usr/bin/perl

# Example 037: Complex pipeline using system() and backticks
# This demonstrates complex pipeline operations with multiple builtins

print "=== Example 037: Complex pipeline ===\n";

# Create test data files
open(my $fh, '>', 'test_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5\n";
print $fh "Bob,30,Manager,87.2\n";
print $fh "Charlie,35,Developer,92.8\n";
print $fh "Diana,28,Designer,88.9\n";
print $fh "Eve,32,Analyst,91.3\n";
print $fh "Frank,29,Engineer,89.7\n";
print $fh "Grace,31,Manager,93.1\n";
print $fh "Henry,27,Developer,86.4\n";
close($fh);

# Complex pipeline 1: Data processing and filtering
print "Complex pipeline 1: Data processing and filtering\n";
print "cat | grep | cut | sort | head\n";
my $pipeline1 = `cat test_data.txt | grep 'Engineer' | cut -d',' -f1,4 | sort -t',' -k2 -nr | head -3`;
print $pipeline1;

# Complex pipeline 2: Text transformation and analysis
print "\nComplex pipeline 2: Text transformation and analysis\n";
print "cat | tr | sed | awk | wc\n";
my $pipeline2 = `cat test_data.txt | tr 'a-z' 'A-Z' | sed 's/,/ | /g' | awk '{print "Name: " $1 " | Age: " $2 " | Role: " $3 " | Score: " $4}' | wc -l`;
print "Total processed lines: $pipeline2";

# Complex pipeline 3: Multi-step data analysis
print "\nComplex pipeline 3: Multi-step data analysis\n";
print "cat | cut | sort | uniq -c | sort -nr\n";

print $pipeline3;

# Complex pipeline 4: File operations and filtering
print "\nComplex pipeline 4: File operations and filtering\n";
print "find | grep | xargs | wc\n";

print $pipeline4;

# Complex pipeline 5: Data aggregation and formatting
print "\nComplex pipeline 5: Data aggregation and formatting\n";
print "cat | awk | sort | head\n";

print $pipeline5;

# Complex pipeline 6: Text processing with multiple filters
print "\nComplex pipeline 6: Text processing with multiple filters\n";
print "cat | grep | sed | tr | sort | uniq\n";

print $pipeline6;

# Complex pipeline 7: Data validation and reporting
print "\nComplex pipeline 7: Data validation and reporting\n";
print "cat | awk | grep | wc\n";

print "High performers: $pipeline7";

# Complex pipeline 8: File system operations
print "\nComplex pipeline 8: File system operations\n";
print "ls | grep | xargs | wc\n";

print "Regular files: $pipeline8";

# Complex pipeline 9: Data transformation and output
print "\nComplex pipeline 9: Data transformation and output\n";
print "cat | cut | sort | uniq | tee\n";

print "Roles saved to file\n";

# Check if roles file was created
if (-f "roles.txt") {
    print "Roles file content:\n";
    
    print $roles_content;
}

# Complex pipeline 10: Error handling and conditional processing
print "\nComplex pipeline 10: Error handling and conditional processing\n";
print "cat | grep | awk | sort | head\n";

print $pipeline10;

# Complex pipeline 11: Multi-file processing
print "\nComplex pipeline 11: Multi-file processing\n";
print "find | xargs | cat | grep | wc\n";

print "Total Engineer mentions: $pipeline11";

# Complex pipeline 12: Data formatting and presentation
print "\nComplex pipeline 12: Data formatting and presentation\n";
print "cat | awk | sort | head | tee\n";

print "Top performers saved to file\n";

# Check if top performers file was created
if (-f "top_performers.txt") {
    print "Top performers file content:\n";
    
    print $top_content;
}

# Clean up
unlink('test_data.txt') if -f 'test_data.txt';
unlink('roles.txt') if -f 'roles.txt';
unlink('top_performers.txt') if -f 'top_performers.txt';

print "=== Example 037 completed successfully ===\n";
