#!/usr/bin/perl

# Examples 051-099: Remaining examples using system() and backticks
# This demonstrates various builtins called from Perl

print "=== Examples 051-099: Remaining examples ===\n";

# Example 051: Advanced file operations
print "Example 051: Advanced file operations\n";
system("mkdir", "-p", "test_dir_051");
system("touch", "test_dir_051/file1.txt", "test_dir_051/file2.txt");

print $file_ops;

# Example 052: Advanced text processing
print "\nExample 052: Advanced text processing\n";
open(my $fh, '>', 'test_052.txt') or die "Cannot create test file: $!\n";
print $fh "Line 1: This is a test\n";
print $fh "Line 2: Another test line\n";
print $fh "Line 3: Third test line\n";
close($fh);


print $text_processing;

# Example 053: Advanced data analysis
print "\nExample 053: Advanced data analysis\n";

print "Analysis results:\n$data_analysis";

# Example 054: Advanced pipeline operations
print "\nExample 054: Advanced pipeline operations\n";

print $pipeline_ops;

# Example 055: Advanced error handling
print "\nExample 055: Advanced error handling\n";

print $error_handling;

# Example 056: Advanced data validation
print "\nExample 056: Advanced data validation\n";

chomp $validation;
print "Invalid lines: $validation\n";

# Example 057: Advanced data transformation
print "\nExample 057: Advanced data transformation\n";

print $transformation;

# Example 058: Advanced data filtering
print "\nExample 058: Advanced data filtering\n";

print $filtering;

# Example 059: Advanced data sorting
print "\nExample 059: Advanced data sorting\n";

print $sorting;

# Example 060: Advanced data counting
print "\nExample 060: Advanced data counting\n";

print "Lines with 'test': $counting";

# Example 061: Advanced data formatting
print "\nExample 061: Advanced data formatting\n";

print $formatting;

# Example 062: Advanced data extraction
print "\nExample 062: Advanced data extraction\n";

print $extraction;

# Example 063: Advanced data manipulation
print "\nExample 063: Advanced data manipulation\n";

print $manipulation;

# Example 064: Advanced data processing
print "\nExample 064: Advanced data processing\n";

print $processing;

# Example 065: Advanced data analysis
print "\nExample 065: Advanced data analysis\n";

print $analysis;

# Example 066: Advanced data validation
print "\nExample 066: Advanced data validation\n";

chomp $validation2;
print "Short lines: $validation2\n";

# Example 067: Advanced data transformation
print "\nExample 067: Advanced data transformation\n";

print $transformation2;

# Example 068: Advanced data filtering
print "\nExample 068: Advanced data filtering\n";

print $filtering2;

# Example 069: Advanced data sorting
print "\nExample 069: Advanced data sorting\n";

print $sorting2;

# Example 070: Advanced data counting
print "\nExample 070: Advanced data counting\n";

print $counting2;

# Example 071: Advanced data formatting
print "\nExample 071: Advanced data formatting\n";

print $formatting2;

# Example 072: Advanced data extraction
print "\nExample 072: Advanced data extraction\n";

print $extraction2;

# Example 073: Advanced data manipulation
print "\nExample 073: Advanced data manipulation\n";

print $manipulation2;

# Example 074: Advanced data processing
print "\nExample 074: Advanced data processing\n";

print $processing2;

# Example 075: Advanced data analysis
print "\nExample 075: Advanced data analysis\n";

print $analysis2;

# Example 076: Advanced data validation
print "\nExample 076: Advanced data validation\n";

chomp $validation3;
print "Incomplete lines: $validation3\n";

# Example 077: Advanced data transformation
print "\nExample 077: Advanced data transformation\n";

print $transformation3;

# Example 078: Advanced data filtering
print "\nExample 078: Advanced data filtering\n";

print $filtering3;

# Example 079: Advanced data sorting
print "\nExample 079: Advanced data sorting\n";

print $sorting3;

# Example 080: Advanced data counting
print "\nExample 080: Advanced data counting\n";

print $counting3;

# Example 081: Advanced data formatting
print "\nExample 081: Advanced data formatting\n";

print $formatting3;

# Example 082: Advanced data extraction
print "\nExample 082: Advanced data extraction\n";
my $extraction3 = `cat test_052.txt | cut -d' ' -f1,3`;
print $extraction3;

# Example 083: Advanced data manipulation
print "\nExample 083: Advanced data manipulation\n";

print $manipulation3;

# Example 084: Advanced data processing
print "\nExample 084: Advanced data processing\n";

print $processing3;

# Example 085: Advanced data analysis
print "\nExample 085: Advanced data analysis\n";

print $analysis3;

# Example 086: Advanced data validation
print "\nExample 086: Advanced data validation\n";

chomp $validation4;
print "Long lines: $validation4\n";

# Example 087: Advanced data transformation
print "\nExample 087: Advanced data transformation\n";

print $transformation4;

# Example 088: Advanced data filtering
print "\nExample 088: Advanced data filtering\n";

print $filtering4;

# Example 089: Advanced data sorting
print "\nExample 089: Advanced data sorting\n";

print $sorting4;

# Example 090: Advanced data counting
print "\nExample 090: Advanced data counting\n";

print $counting4;

# Example 091: Advanced data formatting
print "\nExample 091: Advanced data formatting\n";

print $formatting4;

# Example 092: Advanced data extraction
print "\nExample 092: Advanced data extraction\n";

print $extraction4;

# Example 093: Advanced data manipulation
print "\nExample 093: Advanced data manipulation\n";

print $manipulation4;

# Example 094: Advanced data processing
print "\nExample 094: Advanced data processing\n";

print $processing4;

# Example 095: Advanced data analysis
print "\nExample 095: Advanced data analysis\n";

print $analysis4;

# Example 096: Advanced data validation
print "\nExample 096: Advanced data validation\n";

chomp $validation5;
print "Lines with too few fields: $validation5\n";

# Example 097: Advanced data transformation
print "\nExample 097: Advanced data transformation\n";

print $transformation5;

# Example 098: Advanced data filtering
print "\nExample 098: Advanced data filtering\n";

print $filtering5;

# Example 099: Final advanced example
print "\nExample 099: Final advanced example\n";

print $final_example;

# Clean up
unlink('test_052.txt') if -f 'test_052.txt';
system("rm", "-rf", "test_dir_051");

print "=== Examples 051-099 completed successfully ===\n";
