#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

say "File list:";
# Original bash: ls -1 /tmp 2>/dev/null | head -3
my $output_0 = q{};
say $output_0;
say "---";
say "Count:";
# Original bash: ls /tmp 2>/dev/null | wc -l
my $output_1 = q{};
say $output_1;
say "done";

exit $main_exit_code;


--- Running generated Perl code ---
Exit code: exit status: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0110 seconds
Bash execution time:  0.0600 seconds
Perl is 5.43x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-File list:
-30-orangepi-sysinfo_gen.pl
-VampireDriversFunctions_gen.pl
-_ac_test.pl
----
-Count:
-2582
-done


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
+Can't open perl script "__tmp_run.pl": No such file or directory


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(2)
