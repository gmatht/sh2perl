'AGE:' is not recognized as an internal or external command,
operable program or batch file.
debashc failed with exit code 255: 
#!/usr/bin/perl

# Example 049: Ultimate complex example using system() and backticks
# This demonstrates the most complex operations with builtins called from Perl

print "=== Example 049: Ultimate complex example ===\n";

# Create complex test data
open(my $fh, '>', 'ultimate_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5,New York,USA,2023-01-15\n";
print $fh "Bob,30,Manager,87.2,Los Angeles,USA,2023-02-20\n";
print $fh "Charlie,35,Developer,92.8,Chicago,USA,2023-03-10\n";
print $fh "Diana,28,Designer,88.9,San Francisco,USA,2023-04-05\n";
print $fh "Eve,32,Analyst,91.3,Boston,USA,2023-05-12\n";
print $fh "Frank,29,Engineer,89.7,Seattle,USA,2023-06-18\n";
print $fh "Grace,31,Manager,93.1,Austin,USA,2023-07-25\n";
print $fh "Henry,27,Developer,86.4,Denver,USA,2023-08-30\n";
print $fh "Ivy,33,Designer,94.2,Portland,USA,2023-09-14\n";
print $fh "Jack,26,Analyst,85.8,Miami,USA,2023-10-22\n";
close($fh);

# Ultimate complex pipeline 1: Multi-step data processing with error handling
print "Ultimate complex pipeline 1: Multi-step data processing\n";
print "Processing employee data with advanced transformations...\n";

my $pipeline1 = `cat ultimate_data.txt | grep -E 'Engineer|Developer' | cut -d',' -f1,2,4,5,6 | tr ',' '|' | sort -t'|' -k3 -nr | head -5`;
print "Top technical performers:\n$pipeline1";

# Ultimate complex pipeline 2: Advanced data analysis with multiple filters
print "\nUltimate complex pipeline 2: Advanced data analysis\n";

print "High performers by role and city:\n$pipeline2";

# Ultimate complex pipeline 3: Data validation and quality assurance
print "\nUltimate complex pipeline 3: Data validation\n";

chomp $validation;
if ($validation > 0) {
    print "Data validation failed: $validation invalid lines found\n";
} else {
    print "Data validation passed: All lines have correct format\n";
}

# Ultimate complex pipeline 4: Statistical analysis with multiple metrics
print "\nUltimate complex pipeline 4: Statistical analysis\n";

print "Score statistics:\n$stats";

# Ultimate complex pipeline 5: Geographic and temporal analysis
print "\nUltimate complex pipeline 5: Geographic and temporal analysis\n";

print "Hiring by city and date:\n$geo_temporal";

# Ultimate complex pipeline 6: Data compression and analysis
print "\nUltimate complex pipeline 6: Data compression and analysis\n";

print "Compressed and decompressed lines: $compression";

# Ultimate complex pipeline 7: Multi-file processing with error handling
print "\nUltimate complex pipeline 7: Multi-file processing\n";
system("cp", "ultimate_data.txt", "ultimate_data_copy.txt");

print "Total Engineer mentions across files: $multi_file";

# Ultimate complex pipeline 8: Advanced data transformation and formatting
print "\nUltimate complex pipeline 8: Advanced data transformation\n";
my $transformation = `cat ultimate_data.txt | sed 's/,/ | /g' | tr 'a-z' 'A-Z' | awk '{print "NAME: " $1 " | AGE: " $2 " | ROLE: " $3 " | SCORE: " $4 " | CITY: " $5 " | COUNTRY: " $6 " | DATE: " $7}' | sort | uniq | head -5`;
print "Transformed data:\n$transformation";

# Ultimate complex pipeline 9: Data aggregation and reporting
print "\nUltimate complex pipeline 9: Data aggregation\n";

print "Average scores by role:\n$aggregation";

# Ultimate complex pipeline 10: Complex conditional processing
print "\nUltimate complex pipeline 10: Complex conditional processing\n";

print "High-performing experienced employees:\n$conditional";

# Ultimate complex pipeline 11: Data quality and consistency check
print "\nUltimate complex pipeline 11: Data quality check\n";

chomp $quality;
if ($quality > 0) {
    print "Data quality issue: $quality invalid scores found\n";
} else {
    print "Data quality check passed: All scores are valid\n";
}

# Ultimate complex pipeline 12: Final comprehensive analysis
print "\nUltimate complex pipeline 12: Final comprehensive analysis\n";

print "Most common role-city-date combinations:\n$comprehensive";

# Ultimate complex pipeline 13: Data export and formatting
print "\nUltimate complex pipeline 13: Data export\n";

print "Report saved to file\n";

# Check if report file was created
if (-f "ultimate_report.txt") {
    print "Report file content:\n";
    
    print $report_content;
}

# Ultimate complex pipeline 14: Data backup and cleanup
print "\nUltimate complex pipeline 14: Data backup and cleanup\n";
system("cp", "ultimate_data.txt", "ultimate_data_backup.txt");
system("gzip", "ultimate_data_backup.txt");
if (-f "ultimate_data_backup.txt.gz") {
    print "Backup created and compressed\n";
    
    print "Backup size: $backup_size bytes\n";
}

# Ultimate complex pipeline 15: Final summary and statistics
print "\nUltimate complex pipeline 15: Final summary\n";





print "Final Summary:\n";
print "Total records: $total_lines";
print "Total characters: $total_chars";
print "Unique roles: $unique_roles";
print "Unique cities: $unique_cities";

# Clean up
unlink('ultimate_data.txt') if -f 'ultimate_data.txt';
unlink('ultimate_data_copy.txt') if -f 'ultimate_data_copy.txt';
unlink('ultimate_report.txt') if -f 'ultimate_report.txt';
unlink('ultimate_data_backup.txt.gz') if -f 'ultimate_data_backup.txt.gz';

print "=== Example 049 completed successfully ===\n";
