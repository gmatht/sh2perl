#!/usr/bin/perl

# Example 050: Final example using system() and backticks
# This demonstrates the final complex operations with builtins called from Perl

print "=== Example 050: Final example ===\n";

# Create final test data
open(my $fh, '>', 'final_data.txt') or die "Cannot create test file: $!\n";
print $fh "Project Alpha,2023-01-15,Completed,95.5,Alice,Engineer\n";
print $fh "Project Beta,2023-02-20,In Progress,87.2,Bob,Manager\n";
print $fh "Project Gamma,2023-03-10,Completed,92.8,Charlie,Developer\n";
print $fh "Project Delta,2023-04-05,On Hold,88.9,Diana,Designer\n";
print $fh "Project Epsilon,2023-05-12,Completed,91.3,Eve,Analyst\n";
print $fh "Project Zeta,2023-06-18,In Progress,89.7,Frank,Engineer\n";
print $fh "Project Eta,2023-07-25,Completed,93.1,Grace,Manager\n";
print $fh "Project Theta,2023-08-30,In Progress,86.4,Henry,Developer\n";
print $fh "Project Iota,2023-09-14,Completed,94.2,Ivy,Designer\n";
print $fh "Project Kappa,2023-10-22,On Hold,85.8,Jack,Analyst\n";
close($fh);

# Final complex pipeline 1: Project status analysis
print "Final complex pipeline 1: Project status analysis\n";

print "Project status distribution:\n$status_analysis";

# Final complex pipeline 2: Performance analysis by role
print "\nFinal complex pipeline 2: Performance analysis by role\n";

print "Average performance by role:\n$performance";

# Final complex pipeline 3: Timeline analysis
print "\nFinal complex pipeline 3: Timeline analysis\n";

print "Most common completion dates:\n$timeline";

# Final complex pipeline 4: Data validation and quality check
print "\nFinal complex pipeline 4: Data validation\n";

chomp $validation;
if ($validation > 0) {
    print "Data validation failed: $validation invalid lines found\n";
} else {
    print "Data validation passed: All lines have correct format\n";
}

# Final complex pipeline 5: Statistical analysis
print "\nFinal complex pipeline 5: Statistical analysis\n";

print "Performance statistics:\n$stats";

# Final complex pipeline 6: Data transformation and export
print "\nFinal complex pipeline 6: Data transformation and export\n";

print "Transformed data saved to file\n";

# Check if report file was created
if (-f "final_report.txt") {
    print "Report file content:\n";
    
    print $report_content;
}

# Final complex pipeline 7: Multi-file processing
print "\nFinal complex pipeline 7: Multi-file processing\n";
system("cp", "final_data.txt", "final_data_copy.txt");

print "Total completed projects across files: $multi_file";

# Final complex pipeline 8: Data compression and analysis
print "\nFinal complex pipeline 8: Data compression and analysis\n";

print "Compressed and decompressed lines: $compression";

# Final complex pipeline 9: Advanced data filtering
print "\nFinal complex pipeline 9: Advanced data filtering\n";

print "Top completed projects:\n$filtering";

# Final complex pipeline 10: Data aggregation and reporting
print "\nFinal complex pipeline 10: Data aggregation\n";

print "Average performance by status:\n$aggregation";

# Final complex pipeline 11: Data quality and consistency check
print "\nFinal complex pipeline 11: Data quality check\n";

chomp $quality;
if ($quality > 0) {
    print "Data quality issue: $quality invalid scores found\n";
} else {
    print "Data quality check passed: All scores are valid\n";
}

# Final complex pipeline 12: Comprehensive analysis
print "\nFinal complex pipeline 12: Comprehensive analysis\n";

print "Most common status-role combinations:\n$comprehensive";

# Final complex pipeline 13: Data export and formatting
print "\nFinal complex pipeline 13: Data export\n";

print "Data exported to file\n";

# Check if export file was created
if (-f "final_export.txt") {
    print "Export file content:\n";
    
    print $export_content;
}

# Final complex pipeline 14: Data backup and cleanup
print "\nFinal complex pipeline 14: Data backup and cleanup\n";
system("cp", "final_data.txt", "final_data_backup.txt");
system("gzip", "final_data_backup.txt");
if (-f "final_data_backup.txt.gz") {
    print "Backup created and compressed\n";
    
    print "Backup size: $backup_size bytes\n";
}

# Final complex pipeline 15: Final summary and statistics
print "\nFinal complex pipeline 15: Final summary\n";





print "Final Summary:\n";
print "Total projects: $total_lines";
print "Total characters: $total_chars";
print "Unique statuses: $unique_statuses";
print "Unique roles: $unique_roles";

# Clean up
unlink('final_data.txt') if -f 'final_data.txt';
unlink('final_data_copy.txt') if -f 'final_data_copy.txt';
unlink('final_report.txt') if -f 'final_report.txt';
unlink('final_export.txt') if -f 'final_export.txt';
unlink('final_data_backup.txt.gz') if -f 'final_data_backup.txt.gz';

print "=== Example 050 completed successfully ===\n";
