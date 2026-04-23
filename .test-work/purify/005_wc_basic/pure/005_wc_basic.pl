#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/005_wc_basic.pl" }


print "=== Example 005: Basic wc command ===\n";

open(my $fh, '>', 'test_wc.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
print $fh "Fourth line with more words\n";
print $fh "Fifth and final line\n";
close($fh);

print "Using backticks to call wc:\n";
my $wc_output = do {
use IPC::Open3;
my @wc_args_0 = ('test_wc.txt');
my ($wc_in_0, $wc_out_0, $wc_err_0);
my $wc_pid_0 = open3($wc_in_0, $wc_out_0, $wc_err_0, 'wc', @wc_args_0);
close $wc_in_0 or die "Close failed: $!\n";
my $wc_output_0 = do { local $/ = undef; <$wc_out_0> };
close $wc_out_0 or die "Close failed: $!\n";
waitpid $wc_pid_0, 0;
    $wc_output_0;
}
;
print $wc_output;

print "\nwc -l (line count only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("wc", "-l", "test_wc.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nwc -w (word count only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("wc", "-w", "test_wc.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nwc -c (character count only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("wc", "-c", "test_wc.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nwc with multiple files:\n";
my $multi_wc = do {
use IPC::Open3;
my @wc_args_0 = ('test_wc.txt', 'test_wc.txt');
my ($wc_in_0, $wc_out_0, $wc_err_0);
my $wc_pid_0 = open3($wc_in_0, $wc_out_0, $wc_err_0, 'wc', @wc_args_0);
close $wc_in_0 or die "Close failed: $!\n";
my $wc_output_0 = do { local $/ = undef; <$wc_out_0> };
close $wc_out_0 or die "Close failed: $!\n";
waitpid $wc_pid_0, 0;
    $wc_output_0;
}
;
print $multi_wc;

print "\nwc from stdin (echo | wc):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);use IPC::Open3;{
    my $output_0;
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This is a test line' . "\n";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        use IPC::Open3;
    my @wc_args_0_1 = ();
    my ($wc_in_0_1, $wc_out_0_1, $wc_err_0_1);
    my $wc_pid_0_1 = open3($wc_in_0_1, $wc_out_0_1, $wc_err_0_1, 'wc', @wc_args_0_1);
    print {$wc_in_0_1} $output_0;
    close $wc_in_0_1 or die "Close failed: $!\n";
    my $output_0_1 = do { local $/ = undef; <$wc_out_0_1> };
    close $wc_out_0_1 or die "Close failed: $!\n";
    waitpid $wc_pid_0_1, 0;
    $output_0 = $output_0_1;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nwc -c (bytes):\n";
my $bytes = do {
use IPC::Open3;
my @wc_args_0 = ('-c', 'test_wc.txt');
my ($wc_in_0, $wc_out_0, $wc_err_0);
my $wc_pid_0 = open3($wc_in_0, $wc_out_0, $wc_err_0, 'wc', @wc_args_0);
close $wc_in_0 or die "Close failed: $!\n";
my $wc_output_0 = do { local $/ = undef; <$wc_out_0> };
close $wc_out_0 or die "Close failed: $!\n";
waitpid $wc_pid_0, 0;
    $wc_output_0;
}
;
print $bytes;

print "\nwc -L (maximum line length):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("wc", "-L", "test_wc.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nwc -lwc (lines, words, characters):\n";
my $all_wc = do {
use IPC::Open3;
my @wc_args_0 = ('-lwc', 'test_wc.txt');
my ($wc_in_0, $wc_out_0, $wc_err_0);
my $wc_pid_0 = open3($wc_in_0, $wc_out_0, $wc_err_0, 'wc', @wc_args_0);
close $wc_in_0 or die "Close failed: $!\n";
my $wc_output_0 = do { local $/ = undef; <$wc_out_0> };
close $wc_out_0 or die "Close failed: $!\n";
waitpid $wc_pid_0, 0;
    $wc_output_0;
}
;
print $all_wc;

print "\nwc with totals on multiple files:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("wc", "test_wc.txt", "test_wc.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

unlink('test_wc.txt') if -f 'test_wc.txt';

print "=== Example 005 completed successfully ===\n";
