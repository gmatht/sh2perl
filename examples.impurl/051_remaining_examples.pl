#!/usr/bin/perl

# Examples 051-099: Remaining examples using system() and backticks
# This demonstrates various builtins called from Perl

print "=== Examples 051-099: Remaining examples ===\n";

# Example 051: Advanced file operations
print "Example 051: Advanced file operations\n";
system("mkdir", "-p", "test_dir_051");
system("touch", "test_dir_051/file1.txt", "test_dir_051/file2.txt");
my $file_ops = `find test_dir_051 -name "*.txt" | sort | xargs ls -1`;
print $file_ops;

# Example 052: Advanced text processing
print "\nExample 052: Advanced text processing\n";
open(my $fh, '>', 'test_052.txt') or die "Cannot create test file: $!\n";
print $fh "Line 1: This is a test\n";
print $fh "Line 2: Another test line\n";
print $fh "Line 3: Third test line\n";
close($fh);

my $text_processing = `cat test_052.txt | grep 'test' | tr 'a-z' 'A-Z' | sort`;
print $text_processing;

# Example 053: Advanced data analysis
print "\nExample 053: Advanced data analysis\n";
my $data_analysis = `cat test_052.txt | wc -l && cat test_052.txt | wc -w && cat test_052.txt | wc -c`;
print "Analysis results:\n$data_analysis";

# Example 054: Advanced pipeline operations
print "\nExample 054: Advanced pipeline operations\n";
my $pipeline_ops = `cat test_052.txt | head -2 | tail -1 | tr 'a-z' 'A-Z'`;
print $pipeline_ops;

# Example 055: Advanced error handling
print "\nExample 055: Advanced error handling\n";
my $error_handling = `grep 'nonexistent' test_052.txt 2>&1 || echo 'Pattern not found'`;
print $error_handling;

# Example 056: Advanced data validation
print "\nExample 056: Advanced data validation\n";
my $validation = `cat test_052.txt | awk 'NF != 2 {print "Invalid line: " \$0}' | wc -l`;
chomp $validation;
print "Invalid lines: $validation\n";

# Example 057: Advanced data transformation
print "\nExample 057: Advanced data transformation\n";
my $transformation = `cat test_052.txt | sed 's/Line/ITEM/g' | tr 'a-z' 'A-Z'`;
print $transformation;

# Example 058: Advanced data filtering
print "\nExample 058: Advanced data filtering\n";
my $filtering = `cat test_052.txt | grep -v 'Line 2' | head -2`;
print $filtering;

# Example 059: Advanced data sorting
print "\nExample 059: Advanced data sorting\n";
my $sorting = `cat test_052.txt | sort -r`;
print $sorting;

# Example 060: Advanced data counting
print "\nExample 060: Advanced data counting\n";
my $counting = `cat test_052.txt | grep -c 'test'`;
print "Lines with 'test': $counting";

# Example 061: Advanced data formatting
print "\nExample 061: Advanced data formatting\n";
my $formatting = `cat test_052.txt | awk '{printf "%-10s %s\\n", \$1, \$2}'`;
print $formatting;

# Example 062: Advanced data extraction
print "\nExample 062: Advanced data extraction\n";
my $extraction = `cat test_052.txt | cut -d' ' -f2-`;
print $extraction;

# Example 063: Advanced data manipulation
print "\nExample 063: Advanced data manipulation\n";
my $manipulation = `cat test_052.txt | tr ' ' '|' | sort`;
print $manipulation;

# Example 064: Advanced data processing
print "\nExample 064: Advanced data processing\n";
my $processing = `cat test_052.txt | awk '{print toupper(\$0)}' | sort -r`;
print $processing;

# Example 065: Advanced data analysis
print "\nExample 065: Advanced data analysis\n";
my $analysis = `cat test_052.txt | awk '{sum += length(\$0)} END {print "Total characters: " sum}'`;
print $analysis;

# Example 066: Advanced data validation
print "\nExample 066: Advanced data validation\n";
my $validation2 = `cat test_052.txt | awk 'length(\$0) < 10 {print "Short line: " \$0}' | wc -l`;
chomp $validation2;
print "Short lines: $validation2\n";

# Example 067: Advanced data transformation
print "\nExample 067: Advanced data transformation\n";
my $transformation2 = `cat test_052.txt | sed 's/Line/ITEM/g' | awk '{print "Processed: " \$0}'`;
print $transformation2;

# Example 068: Advanced data filtering
print "\nExample 068: Advanced data filtering\n";
my $filtering2 = `cat test_052.txt | grep 'Line [13]' | head -2`;
print $filtering2;

# Example 069: Advanced data sorting
print "\nExample 069: Advanced data sorting\n";
my $sorting2 = `cat test_052.txt | sort -k2`;
print $sorting2;

# Example 070: Advanced data counting
print "\nExample 070: Advanced data counting\n";
my $counting2 = `cat test_052.txt | awk '{print NF}' | sort -n | uniq -c`;
print $counting2;

# Example 071: Advanced data formatting
print "\nExample 071: Advanced data formatting\n";
my $formatting2 = `cat test_052.txt | awk '{printf "%-5s %s\\n", NR, \$0}'`;
print $formatting2;

# Example 072: Advanced data extraction
print "\nExample 072: Advanced data extraction\n";
my $extraction2 = `cat test_052.txt | cut -d':' -f2 | tr -d ' '`;
print $extraction2;

# Example 073: Advanced data manipulation
print "\nExample 073: Advanced data manipulation\n";
my $manipulation2 = `cat test_052.txt | tr 'a-z' 'A-Z' | tr ' ' '_'`;
print $manipulation2;

# Example 074: Advanced data processing
print "\nExample 074: Advanced data processing\n";
my $processing2 = `cat test_052.txt | awk '{print length(\$0) " " \$0}' | sort -n`;
print $processing2;

# Example 075: Advanced data analysis
print "\nExample 075: Advanced data analysis\n";
my $analysis2 = `cat test_052.txt | awk '{sum += NF} END {print "Total words: " sum}'`;
print $analysis2;

# Example 076: Advanced data validation
print "\nExample 076: Advanced data validation\n";
my $validation3 = `cat test_052.txt | awk 'NF < 3 {print "Incomplete line: " \$0}' | wc -l`;
chomp $validation3;
print "Incomplete lines: $validation3\n";

# Example 077: Advanced data transformation
print "\nExample 077: Advanced data transformation\n";
my $transformation3 = `cat test_052.txt | sed 's/Line/ITEM/g' | awk '{print "Item " NR ": " \$0}'`;
print $transformation3;

# Example 078: Advanced data filtering
print "\nExample 078: Advanced data filtering\n";
my $filtering3 = `cat test_052.txt | grep -E 'Line [1-2]' | head -2`;
print $filtering3;

# Example 079: Advanced data sorting
print "\nExample 079: Advanced data sorting\n";
my $sorting3 = `cat test_052.txt | sort -k3`;
print $sorting3;

# Example 080: Advanced data counting
print "\nExample 080: Advanced data counting\n";
my $counting3 = `cat test_052.txt | awk '{print \$1}' | sort | uniq -c`;
print $counting3;

# Example 081: Advanced data formatting
print "\nExample 081: Advanced data formatting\n";
my $formatting3 = `cat test_052.txt | awk '{printf "%-10s %-10s %s\\n", \$1, \$2, \$3}'`;
print $formatting3;

# Example 082: Advanced data extraction
print "\nExample 082: Advanced data extraction\n";
my $extraction3 = `cat test_052.txt | cut -d' ' -f1,3`;
print $extraction3;

# Example 083: Advanced data manipulation
print "\nExample 083: Advanced data manipulation\n";
my $manipulation3 = `cat test_052.txt | tr 'a-z' 'A-Z' | tr ' ' '|' | sort`;
print $manipulation3;

# Example 084: Advanced data processing
print "\nExample 084: Advanced data processing\n";
my $processing3 = `cat test_052.txt | awk '{print toupper(\$0)}' | sort -r | head -2`;
print $processing3;

# Example 085: Advanced data analysis
print "\nExample 085: Advanced data analysis\n";
my $analysis3 = `cat test_052.txt | awk '{sum += length(\$0)} END {print "Average line length: " sum/NR}'`;
print $analysis3;

# Example 086: Advanced data validation
print "\nExample 086: Advanced data validation\n";
my $validation4 = `cat test_052.txt | awk 'length(\$0) > 20 {print "Long line: " \$0}' | wc -l`;
chomp $validation4;
print "Long lines: $validation4\n";

# Example 087: Advanced data transformation
print "\nExample 087: Advanced data transformation\n";
my $transformation4 = `cat test_052.txt | sed 's/Line/ITEM/g' | awk '{print "Processed " NR ": " \$0}'`;
print $transformation4;

# Example 088: Advanced data filtering
print "\nExample 088: Advanced data filtering\n";
my $filtering4 = `cat test_052.txt | grep -v 'Line 2' | head -2`;
print $filtering4;

# Example 089: Advanced data sorting
print "\nExample 089: Advanced data sorting\n";
my $sorting4 = `cat test_052.txt | sort -k2 -r`;
print $sorting4;

# Example 090: Advanced data counting
print "\nExample 090: Advanced data counting\n";
my $counting4 = `cat test_052.txt | awk '{print \$2}' | sort | uniq -c`;
print $counting4;

# Example 091: Advanced data formatting
print "\nExample 091: Advanced data formatting\n";
my $formatting4 = `cat test_052.txt | awk '{printf "%-5s %-10s %s\\n", NR, \$1, \$2}'`;
print $formatting4;

# Example 092: Advanced data extraction
print "\nExample 092: Advanced data extraction\n";
my $extraction4 = `cat test_052.txt | cut -d' ' -f2- | tr ' ' '_'`;
print $extraction4;

# Example 093: Advanced data manipulation
print "\nExample 093: Advanced data manipulation\n";
my $manipulation4 = `cat test_052.txt | tr 'a-z' 'A-Z' | tr ' ' '|' | sort -r`;
print $manipulation4;

# Example 094: Advanced data processing
print "\nExample 094: Advanced data processing\n";
my $processing4 = `cat test_052.txt | awk '{print toupper(\$0)}' | sort | head -2`;
print $processing4;

# Example 095: Advanced data analysis
print "\nExample 095: Advanced data analysis\n";
my $analysis4 = `cat test_052.txt | awk '{sum += NF} END {print "Average words per line: " sum/NR}'`;
print $analysis4;

# Example 096: Advanced data validation
print "\nExample 096: Advanced data validation\n";
my $validation5 = `cat test_052.txt | awk 'NF < 2 {print "Too few fields: " \$0}' | wc -l`;
chomp $validation5;
print "Lines with too few fields: $validation5\n";

# Example 097: Advanced data transformation
print "\nExample 097: Advanced data transformation\n";
my $transformation5 = `cat test_052.txt | sed 's/Line/ITEM/g' | awk '{print "Item " NR ": " \$0}' | sort`;
print $transformation5;

# Example 098: Advanced data filtering
print "\nExample 098: Advanced data filtering\n";
my $filtering5 = `cat test_052.txt | grep -E 'Line [1-3]' | head -3`;
print $filtering5;

# Example 099: Final advanced example
print "\nExample 099: Final advanced example\n";
my $final_example = `cat test_052.txt | awk '{print toupper(\$0)}' | tr ' ' '|' | sort -r | head -3`;
print $final_example;

# Clean up
unlink('test_052.txt') if -f 'test_052.txt';
system("rm", "-rf", "test_dir_051");

print "=== Examples 051-099 completed successfully ===\n";
