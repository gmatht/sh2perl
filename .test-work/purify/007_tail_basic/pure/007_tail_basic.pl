#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/007_tail_basic.pl" }


print "=== Example 007: Basic tail command ===\n";

open(my $fh, '>', 'test_tail.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

print "Using backticks to call tail (default 10 lines):\n";
my $tail_output = do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail test_tail.txt'; qx{$tail_cmd}; }
;
print $tail_output;

print "\ntail -n 5 (last 5 lines):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("tail", "-n", "5", "test_tail.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntail -n 3 (last 3 lines):\n";
my $tail3 = do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 3 test_tail.txt'; qx{$tail_cmd}; }
;
print $tail3;

print "\ntail -n 1 (last line only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("tail", "-n", "1", "test_tail.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntail -n 15 (more than available):\n";
my $tail15 = do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 15 test_tail.txt'; qx{$tail_cmd}; }
;
print $tail15;

print "\ntail -c 50 (last 50 characters):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("tail", "-c", "50", "test_tail.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntail -c 100 (last 100 characters):\n";
my $tail_bytes = do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -c 100 test_tail.txt'; qx{$tail_cmd}; }
;
print $tail_bytes;

print "\ntail from stdin (echo | tail):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    my @tail_lines = ();
    $output_0 .= "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        my @lines = split /\n/msx, $output_0;
    my $num_lines = 3;
    if ($num_lines > scalar @lines) {
    $num_lines = scalar @lines;
    }
    my $start_index = scalar @lines - $num_lines;
    if ($start_index < 0) { $start_index = 0; }
    my @result = @lines[$start_index..$#lines];
    $output_0 = join "\n", @result;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\ntail -f simulation (follow mode):\n";
my $tail_follow = do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 3 test_tail.txt'; qx{$tail_cmd}; }
;
print $tail_follow;

print "\ntail -q (quiet mode, no filename):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("tail", "-q", "test_tail.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

unlink('test_tail.txt') if -f 'test_tail.txt';

print "=== Example 007 completed successfully ===\n";
