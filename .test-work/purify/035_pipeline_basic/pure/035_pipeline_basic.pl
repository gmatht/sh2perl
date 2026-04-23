#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/035_pipeline_basic.pl" }


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
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "grep", "a", "|", "wc", "-l"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nPipeline with head and tail:\n";
my $pipeline_head_tail = do { my $pipeline_cmd = 'cat test_pipeline.txt | head -5 | tail -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_head_tail;

print "\nPipeline with sed and awk:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "sed", "s/a/A/g", "|", "awk", "{print toupper($0)}"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nPipeline with cut and paste:\n";
my $pipeline_cut_paste = do { my $pipeline_cmd = q{echo '1,2,3
4,5,6
7,8,9' | cut -d, -f 1,3 | paste - -}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_cut_paste;

print "\nPipeline with tr and sort:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "tr", "a-z", "A-Z", "|", "sort"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nPipeline with uniq and wc:\n";
my $pipeline_uniq_wc = do { my $pipeline_cmd = 'cat test_pipeline.txt | sort | uniq | wc -l'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Unique lines: $pipeline_uniq_wc";

print "\nPipeline with grep and head:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "grep", "e", "|", "head", "-2"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nPipeline with tail and grep:\n";
my $pipeline_tail_grep = do { my $pipeline_cmd = 'cat test_pipeline.txt | tail -5 | grep a'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $pipeline_tail_grep;

print "\nPipeline with multiple filters:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "grep", "a", "|", "sort", "|", "head", "-3"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nPipeline with error handling:\n";
my $pipeline_error = do { my $pipeline_cmd = 'cat test_pipeline.txt | grep x | wc -l 2> /dev/null'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Lines with 'x': $pipeline_error";

print "\nPipeline with tee:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cat", "test_pipeline.txt", "|", "grep", "a", "|", "tee", "pipeline_output.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

if (-f "pipeline_output.txt") {
    print "Pipeline output file created\n";
    my $output_content = do { open my $fh, '<', 'pipeline_output.txt' or die 'cat: ' . 'pipeline_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "Output content: $output_content";
}

unlink('test_pipeline.txt') if -f 'test_pipeline.txt';
unlink('pipeline_output.txt') if -f 'pipeline_output.txt';

print "=== Example 035 completed successfully ===\n";
