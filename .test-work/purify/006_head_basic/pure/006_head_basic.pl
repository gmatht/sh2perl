#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/006_head_basic.pl" }


print "=== Example 006: Basic head command ===\n";

open(my $fh, '>', 'test_head.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

print "Using backticks to call head (default 10 lines):\n";
my $head_output = do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head test_head.txt'; qx{$head_cmd}; }
;
print $head_output;

print "\nhead -n 5 (first 5 lines):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('head', '-n', '5', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nhead -n 3 (first 3 lines):\n";
my $head3 = do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -n 3 test_head.txt'; qx{$head_cmd}; }
;
print $head3;

print "\nhead -n 1 (first line only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('head', '-n', '1', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nhead -n 15 (more than available):\n";
my $head15 = do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -n 15 test_head.txt'; qx{$head_cmd}; }
;
print $head15;

print "\nhead -c 50 (first 50 characters):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('head', '-c', '50', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nhead -c 100 (first 100 characters):\n";
my $head_bytes = do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -c 100 test_head.txt'; qx{$head_cmd}; }
;
print $head_bytes;

print "\nhead from stdin (echo | head):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        my $num_lines       = 3;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_0;
    my $pos             = 0;
    while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) {
    $line_end = length $input;
    }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
    }
    $output_0 = $result;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nhead -q (quiet mode, no filename):\n";
my $head_quiet = do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -q test_head.txt'; qx{$head_cmd}; }
;
print $head_quiet;

print "\nhead -v (verbose mode, with filename):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('head', '-v', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

unlink('test_head.txt') if -f 'test_head.txt';

print "=== Example 006 completed successfully ===\n";
