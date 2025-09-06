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
my $pipeline3 = `cat test_data.txt | cut -d',' -f3 | sort | uniq -c | sort -nr`;
print $pipeline3;

# Complex pipeline 4: File operations and filtering
print "\nComplex pipeline 4: File operations and filtering\n";
print "find | grep | xargs | wc\n";
my $pipeline4 = `find . -name "*.txt" | grep test | xargs wc -l`;
print $pipeline4;

# Complex pipeline 5: Data aggregation and formatting
print "\nComplex pipeline 5: Data aggregation and formatting\n";
print "cat | awk | sort | head\n";
my $pipeline5 = `cat test_data.txt | awk -F',' '{print $2 "," $4}' | sort -t',' -k2 -nr | head -3`;
print $pipeline5;

# Complex pipeline 6: Text processing with multiple filters
print "\nComplex pipeline 6: Text processing with multiple filters\n";
print "cat | grep | sed | tr | sort | uniq\n";
my $pipeline6 = `cat test_data.txt | grep -v 'Manager' | sed 's/,/ /g' | tr 'a-z' 'A-Z' | sort | uniq`;
print $pipeline6;

# Complex pipeline 7: Data validation and reporting
print "\nComplex pipeline 7: Data validation and reporting\n";
print "cat | awk | grep | wc\n";
my $pipeline7 = `cat test_data.txt | awk -F',' '$4 > 90 {print $1 " has high score: " $4}' | wc -l`;
print "High performers: $pipeline7";

# Complex pipeline 8: File system operations
print "\nComplex pipeline 8: File system operations\n";
print "ls | grep | xargs | wc\n";
my $pipeline8 = `ls -la | grep '^-' | xargs -I {} echo "File: {}" | wc -l`;
print "Regular files: $pipeline8";

# Complex pipeline 9: Data transformation and output
print "\nComplex pipeline 9: Data transformation and output\n";
print "cat | cut | sort | uniq | tee\n";
my $pipeline9 = `cat test_data.txt | cut -d',' -f3 | sort | uniq | tee roles.txt`;
print "Roles saved to file\n";

# Check if roles file was created
if (-f "roles.txt") {
    print "Roles file content:\n";
    my $roles_content = `cat roles.txt`;
    print $roles_content;
}

# Complex pipeline 10: Error handling and conditional processing
print "\nComplex pipeline 10: Error handling and conditional processing\n";
print "cat | grep | awk | sort | head\n";
my $pipeline10 = `cat test_data.txt | grep 'Engineer\\|Developer' | awk -F',' '{print $1 " (" $3 "): " $4}' | sort -k3 -nr | head -3`;
print $pipeline10;

# Complex pipeline 11: Multi-file processing
print "\nComplex pipeline 11: Multi-file processing\n";
print "find | xargs | cat | grep | wc\n";
my $pipeline11 = `find . -name "*.txt" | xargs cat | grep -c 'Engineer'`;
print "Total Engineer mentions: $pipeline11";

# Complex pipeline 12: Data formatting and presentation
print "\nComplex pipeline 12: Data formatting and presentation\n";
print "cat | awk | sort | head | tee\n";
my $pipeline12 = `cat test_data.txt | awk -F',' '{printf "%-10s %3d %-10s %5.1f\\n", $1, $2, $3, $4}' | sort -k4 -nr | head -5 | tee top_performers.txt`;
print "Top performers saved to file\n";

# Check if top performers file was created
if (-f "top_performers.txt") {
    print "Top performers file content:\n";
    my $top_content = `cat top_performers.txt`;
    print $top_content;
}

# Clean up
unlink('test_data.txt') if -f 'test_data.txt';
unlink('roles.txt') if -f 'roles.txt';
unlink('top_performers.txt') if -f 'top_performers.txt';

print "=== Example 037 completed successfully ===\n";
