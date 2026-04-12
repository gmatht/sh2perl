Running shell script: examples/000__04d_system_utilities.sh
Generated Perl code:
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

print "=== System Utilities ===\n";
my $formatted_date = do { use POSIX qw(strftime); strftime('%Y-%m-%d %H', localtime); };
my $timestamp = do { use POSIX qw(strftime); strftime('%rms', localtime); };
print "Timestamp: $timestamp\n";
print "Formatted date: $formatted_date\n";
my $time_result = do {
    my $cmd_result_1 =  my ($in_0, $out_0, $err_0);
    my $pid_0 = open3($in_0, $out_0, $err_0, 'time', 'sleep', q{1});
    close $in_0 or croak 'Close failed: $!';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $!';
    waitpid $pid_0, 0;
    $result_0;
    chomp $cmd_result_1;
    $cmd_result_1;
};
print "Time result: $time_result\n";
my $sleep_duration = (q{2});
print "Sleeping for $sleep_duration seconds...\n";
use Time::HiRes qw(sleep);
sleep $sleep_duration;
my $bash_path = do { my $command; my $found; my $result; my $dir; my $full_path; $command = 'bash'; $found = 0; $result = q{}; foreach my $dir (split /:/msx, $ENV{PATH}) { $full_path = "$dir/$command"; if (-x $full_path) { $result = $full_path; $found = 1; last; } } $result; };
print "Bash path: $bash_path\n";
my $yes_result = do {
    my $cmd_result_4 = my $head_line_count = 0;
my $cmd_result_3 = q{};
my $output_0 = q{};
for (my $i = 0; $i < 3; $i++) {
    my $line = "Hello";
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
        $output_0 .= $line . "\n";
        ++$head_line_count;
    } else {
        $line = q{}; # Clear line to prevent printing
    }
}
$cmd_result_3 = $output_0;
;
    chomp $cmd_result_4;
    $cmd_result_4;
};
print "Yes command result:\n";
print $yes_result;
if (!($yes_result =~ /\n$/msx)) { print "\n"; }
print "=== System Utilities Complete ===\n";


--- Running generated Perl code ---
Exit code: exit code: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0634 seconds
Bash execution time:  3.4617 seconds
Perl is 54.58x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-=== System Utilities ===
-Timestamp: 10:31:45 PMms
-Formatted date: 2025-09-21 22
-Time result: 
-Sleeping for 2 seconds...
-Bash path: /usr/bin/bash
-Yes command result:
-Hello
-Hello
-Hello
-=== System Utilities Complete ===


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
-
+Can't open perl script "__tmp_run.pl": No such file or directory
-real\x090m1.043s
-user\x090m0.015s
-sys\x090m0.015s


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(2)
