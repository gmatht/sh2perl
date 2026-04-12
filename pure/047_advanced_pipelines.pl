'AGE:' is not recognized as an internal or external command,
operable program or batch file.
debashc failed with exit code 255: 
#!/usr/bin/perl

# Example 047: Advanced pipelines using system() and backticks
# This demonstrates advanced pipeline operations with builtins

print "=== Example 047: Advanced pipelines ===\n";

# Create complex test data
open(my $fh, '>', 'advanced_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5,New York,USA\n";
print $fh "Bob,30,Manager,87.2,Los Angeles,USA\n";
print $fh "Charlie,35,Developer,92.8,Chicago,USA\n";
print $fh "Diana,28,Designer,88.9,San Francisco,USA\n";
print $fh "Eve,32,Analyst,91.3,Boston,USA\n";
print $fh "Frank,29,Engineer,89.7,Seattle,USA\n";
print $fh "Grace,31,Manager,93.1,Austin,USA\n";
print $fh "Henry,27,Developer,86.4,Denver,USA\n";
print $fh "Ivy,33,Designer,94.2,Portland,USA\n";
print $fh "Jack,26,Analyst,85.8,Miami,USA\n";
close($fh);

# Advanced pipeline 1: Multi-step data transformation
print "Advanced pipeline 1: Multi-step data transformation\n";
print "cat | grep | cut | tr | sort | uniq | wc\n";
my $pipeline1 = `cat advanced_data.txt | grep 'Engineer\\|Developer' | cut -d',' -f1,3,4 | tr ',' '|' | sort -t'|' -k3 -nr | uniq | wc -l`;
print "Processed records: $pipeline1";

# Advanced pipeline 2: Complex data analysis
print "\nAdvanced pipeline 2: Complex data analysis\n";
print "cat | awk | sort | uniq -c | sort -nr | head\n";

print "Role distribution:\n$pipeline2";

# Advanced pipeline 3: Data validation and filtering
print "\nAdvanced pipeline 3: Data validation and filtering\n";
print "cat | awk | grep | sort | head\n";

print "Top performers:\n$pipeline3";

# Advanced pipeline 4: Geographic analysis
print "\nAdvanced pipeline 4: Geographic analysis\n";
print "cat | cut | sort | uniq -c | sort -nr\n";

print "City distribution:\n$pipeline4";

# Advanced pipeline 5: Statistical analysis
print "\nAdvanced pipeline 5: Statistical analysis\n";
print "cat | cut | sort -n | awk\n";

print "Score statistics:\n$pipeline5";

# Advanced pipeline 6: Data formatting and presentation
print "\nAdvanced pipeline 6: Data formatting and presentation\n";
print "cat | awk | sort | head | tee\n";

print "Formatted output saved to file\n";

# Check if formatted output file was created
if (-f "formatted_output.txt") {
    print "Formatted output file content:\n";
    
    print $formatted_content;
}

# Advanced pipeline 7: Multi-file processing
print "\nAdvanced pipeline 7: Multi-file processing\n";
print "find | xargs | cat | grep | wc\n";
system("cp", "advanced_data.txt", "advanced_data_copy.txt");

print "Total Engineer mentions: $pipeline7";

# Advanced pipeline 8: Data compression and analysis
print "\nAdvanced pipeline 8: Data compression and analysis\n";
print "cat | gzip | zcat | wc\n";

print "Compressed and decompressed lines: $pipeline8";

# Advanced pipeline 9: Error handling and recovery
print "\nAdvanced pipeline 9: Error handling and recovery\n";
print "cat | grep | awk | sort | head\n";

print "Top performers by role:\n$pipeline9";

# Advanced pipeline 10: Complex data transformation
print "\nAdvanced pipeline 10: Complex data transformation\n";
print "cat | sed | tr | awk | sort | uniq\n";
my $pipeline10 = `cat advanced_data.txt | sed 's/,/ | /g' | tr 'a-z' 'A-Z' | awk '{print "NAME: " $1 " | AGE: " $2 " | ROLE: " $3 " | SCORE: " $4 " | CITY: " $5}' | sort | uniq | head -3`;
print "Transformed data:\n$pipeline10";

# Advanced pipeline 11: Data aggregation and reporting
print "\nAdvanced pipeline 11: Data aggregation and reporting\n";
print "cat | awk | sort | uniq -c | sort -nr | head\n";

print "Role-City combinations:\n$pipeline11";

# Advanced pipeline 12: Data validation and quality check
print "\nAdvanced pipeline 12: Data validation and quality check\n";
print "cat | awk | grep | wc\n";

chomp $pipeline12;
if ($pipeline12 > 0) {
    print "Data validation failed: $pipeline12 invalid lines found\n";
} else {
    print "Data validation passed: All lines have correct format\n";
}

# Clean up
unlink('advanced_data.txt') if -f 'advanced_data.txt';
unlink('advanced_data_copy.txt') if -f 'advanced_data_copy.txt';
unlink('formatted_output.txt') if -f 'formatted_output.txt';

print "=== Example 047 completed successfully ===\n";
