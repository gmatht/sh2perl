Running shell script: examples/065_yes_head_while.sh
Generated Perl code:
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

my $output_0 = q{};
my $output_printed_0;
my $head_line_count = 0;
while (1) {
    my $line = 'Line:LINE';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 100) {
    $output_0 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

exit $main_exit_code;


--- Running generated Perl code ---
Exit code: exit status: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0019 seconds
Bash execution time:  0.1445 seconds
Perl is 77.16x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-Line:1
-Line:2
-Line:3
-Line:4
-Line:5
-Line:6
-Line:7
-Line:8
-Line:9
-Line:10
-Line:11
-Line:12
-Line:13
-Line:14
-Line:15
-Line:16
-Line:17
-Line:18
-Line:19
-Line:20
-Line:21
-Line:22
-Line:23
-Line:24
-Line:25
-Line:26
-Line:27
-Line:28
-Line:29
-Line:30
-Line:31
-Line:32
-Line:33
-Line:34
-Line:35
-Line:36
-Line:37
-Line:38
-Line:39
-Line:40
-Line:41
-Line:42
-Line:43
-Line:44
-Line:45
-Line:46
-Line:47
-Line:48
-Line:49
-Line:50
-Line:51
-Line:52
-Line:53
-Line:54
-Line:55
-Line:56
-Line:57
-Line:58
-Line:59
-Line:60
-Line:61
-Line:62
-Line:63
-Line:64
-Line:65
-Line:66
-Line:67
-Line:68
-Line:69
-Line:70
-Line:71
-Line:72
-Line:73
-Line:74
-Line:75
-Line:76
-Line:77
-Line:78
-Line:79
-Line:80
-Line:81
-Line:82
-Line:83
-Line:84
-Line:85
-Line:86
-Line:87
-Line:88
-Line:89
-Line:90
-Line:91
-Line:92
-Line:93
-Line:94
-Line:95
-Line:96
-Line:97
-Line:98
-Line:99
-Line:100


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
+Can't open perl script "__tmp_run.pl": No such file or directory


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(2)
