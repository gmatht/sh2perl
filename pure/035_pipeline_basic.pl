#!/usr/bin/perl

# Example 035: Basic pipeline using system() and backticks
# This demonstrates pipeline operations with builtins called from Perl

print "=== Example 035: Basic pipeline ===\n";

# Create test file first
open(my $fh, '>', 'test_pipeline.txt') or die "Cannot create test file: $!\n";
print $fh "apple\n";
print $fh "banana\n";
print $fh "cherry\n";
print $fh "date\n";
print $fh "elderberry\n";
print $fh "fig\n";
print $fh "grape\n";
close($fh);

# Simple pipeline using backticks
print "Using backticks to call pipeline (cat | grep | sort):\n";

print $pipeline_output;

# Pipeline with multiple commands using system()
print "\nPipeline with multiple commands (cat | grep | wc):\n";
system("cat", "test_pipeline.txt", "|", "grep", "a", "|", "wc", "-l");

# Pipeline with head and tail using backticks
print "\nPipeline with head and tail:\n";

print $pipeline_head_tail;

# Pipeline with sed and awk using system()
print "\nPipeline with sed and awk:\n";
system("cat", "test_pipeline.txt", "|", "sed", "s/a/A/g", "|", "awk", "{print toupper($0)}");

# Pipeline with cut and paste using backticks
print "\nPipeline with cut and paste:\n";
my $pipeline_cut_paste = `echo '1,2,3\n4,5,6\n7,8,9' | cut -d',' -f1,3 | paste - -`;
print $pipeline_cut_paste;

# Pipeline with tr and sort using system()
print "\nPipeline with tr and sort:\n";
system("cat", "test_pipeline.txt", "|", "tr", "a-z", "A-Z", "|", "sort");

# Pipeline with uniq and wc using backticks
print "\nPipeline with uniq and wc:\n";

print "Unique lines: $pipeline_uniq_wc";

# Pipeline with grep and head using system()
print "\nPipeline with grep and head:\n";
system("cat", "test_pipeline.txt", "|", "grep", "e", "|", "head", "-2");

# Pipeline with tail and grep using backticks
print "\nPipeline with tail and grep:\n";

print $pipeline_tail_grep;

# Pipeline with multiple filters using system()
print "\nPipeline with multiple filters:\n";
system("cat", "test_pipeline.txt", "|", "grep", "a", "|", "sort", "|", "head", "-3");

# Pipeline with error handling using backticks
print "\nPipeline with error handling:\n";

print "Lines with 'x': $pipeline_error";

# Pipeline with tee using system()
print "\nPipeline with tee:\n";
system("cat", "test_pipeline.txt", "|", "grep", "a", "|", "tee", "pipeline_output.txt");

# Check if output file was created
if (-f "pipeline_output.txt") {
    print "Pipeline output file created\n";
    
    print "Output content: $output_content";
}

# Clean up
unlink('test_pipeline.txt') if -f 'test_pipeline.txt';
unlink('pipeline_output.txt') if -f 'pipeline_output.txt';

print "=== Example 035 completed successfully ===\n";
