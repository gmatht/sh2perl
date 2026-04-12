#!/usr/bin/perl


print "=== Example 035: Basic pipeline ===\n";

open(my $fh, '>', 'test_pipeline.txt') or die "Cannot create test file: $!\n";
print $fh "apple\n";
print $fh "banana\n";
print $fh "cherry\n";
print $fh "date\n";
print $fh "elderberry\n";
print $fh "fig\n";
print $fh "grape\n";
close($fh);

print "Using backticks to call pipeline (cat | grep | sort):\n";
my $pipeline_output = do { my $pipeline_cmd = 'cat test_pipeline.txt | grep a | sort'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_output;

print "\nPipeline with multiple commands (cat | grep | wc):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print do { my $cat_cmd = 'cat test_pipeline.txt | grep a | wc -l'; qx{$cat_cmd}; };

};

print "\nPipeline with head and tail:\n";
my $pipeline_head_tail = do { my $pipeline_cmd = 'cat test_pipeline.txt | head -5 | tail -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_head_tail;

print "\nPipeline with sed and awk:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print (do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'sed' or die 'cat: ' . 'sed' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 's/a/A/g' or die 'cat: ' . 's/a/A/g' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'awk' or die 'cat: ' . 'awk' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', '{print toupper($0)}' or die 'cat: ' . '{print toupper($0)}' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; });

};

print "\nPipeline with cut and paste:\n";
my $pipeline_cut_paste = do { my $pipeline_cmd = "echo '1,2,3\n4,5,6\n7,8,9' | cut -d, -f 1,3 | paste - -"; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_cut_paste;

print "\nPipeline with tr and sort:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print (do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'tr' or die 'cat: ' . 'tr' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'a-z' or die 'cat: ' . 'a-z' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'A-Z' or die 'cat: ' . 'A-Z' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'sort' or die 'cat: ' . 'sort' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; });

};

print "\nPipeline with uniq and wc:\n";
my $pipeline_uniq_wc = do { my $pipeline_cmd = 'cat test_pipeline.txt | sort | uniq | wc -l'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Unique lines: $pipeline_uniq_wc";

print "\nPipeline with grep and head:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print do { my $cat_cmd = 'cat test_pipeline.txt | grep e | head -2'; qx{$cat_cmd}; };

};

print "\nPipeline with tail and grep:\n";
my $pipeline_tail_grep = do { my $pipeline_cmd = 'cat test_pipeline.txt | tail -5 | grep a'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_tail_grep;

print "\nPipeline with multiple filters:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print do { my $cat_cmd = 'cat test_pipeline.txt | grep a | sort | head -3'; qx{$cat_cmd}; };

};

print "\nPipeline with error handling:\n";
my $pipeline_error = do { my $pipeline_cmd = 'cat test_pipeline.txt | grep x'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Lines with 'x': $pipeline_error";

print "\nPipeline with tee:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print (do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'grep' or die 'cat: ' . 'grep' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{a} or die 'cat: ' . q{a} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', q{|} or die 'cat: ' . q{|} . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'tee' or die 'cat: ' . 'tee' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } . do { open my $fh, '<', 'pipeline_output.txt' or die 'cat: ' . 'pipeline_output.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; });

};

if (-f "pipeline_output.txt") {
    print "Pipeline output file created\n";
    my $output_content = do { open my $fh, '<', 'pipeline_output.txt' or die 'cat: ' . 'pipeline_output.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; }
;
    print "Output content: $output_content";
}

unlink('test_pipeline.txt') if -f 'test_pipeline.txt';
unlink('pipeline_output.txt') if -f 'pipeline_output.txt';

print "=== Example 035 completed successfully ===\n";
