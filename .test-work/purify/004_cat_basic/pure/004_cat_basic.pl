#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/004_cat_basic.pl" }


print "=== Example 004: Basic cat command ===\n";

open(my $fh1, '>', 'test_file1.txt') or die "Cannot create test file: $!\n";
print $fh1 "Line 1: This is test file 1\n";
print $fh1 "Line 2: Created for cat demonstration\n";
print $fh1 "Line 3: Multiple lines of content\n";
close($fh1);

open(my $fh2, '>', 'test_file2.txt') or die "Cannot create test file: $!\n";
print $fh2 "Line 1: This is test file 2\n";
print $fh2 "Line 2: Another file for cat demo\n";
print $fh2 "Line 3: Different content here\n";
close($fh2);

print "Using backticks to call cat:\n";
my $cat_output = do { open my $fh, '<', 'test_file1.txt' or die 'cat: ' . 'test_file1.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
print $cat_output;

print "\ncat with multiple files using " . "sys" . "tem" . "():\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('cat', 'test_file1.txt', 'test_file2.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ncat with redirection (cat file1 file2 > combined.txt):\n";
my $combined = do { my $command = 'cat test_file1.txt test_file2.txt > combined.txt'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
if (-f "combined.txt") {
    print "Combined file created successfully\n";
    my $combined_content = do { open my $fh, '<', 'combined.txt' or die 'cat: ' . 'combined.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "Combined content:\n$combined_content";
}

print "\ncat from stdin (echo | cat):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This is from stdin' . "\n";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\ncat with line numbers (cat -n):\n";
my $numbered = do { my $cat_cmd = 'cat -n test_file1.txt'; qx{$cat_cmd}; }
;
print $numbered;

print "\ncat with non-printing characters (cat -v):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('cat', '-v', 'test_file1.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ncat with squeeze blank lines (cat -s):\n";
my $squeezed = do { my $cat_cmd = 'cat -s test_file1.txt'; qx{$cat_cmd}; }
;
print $squeezed;

print "\ncat with tabs (cat -T):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('cat', '-T', 'test_file1.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

unlink('test_file1.txt') if -f 'test_file1.txt';
unlink('test_file2.txt') if -f 'test_file2.txt';
unlink('combined.txt') if -f 'combined.txt';

print "=== Example 004 completed successfully ===\n";
