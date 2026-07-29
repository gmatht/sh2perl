#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

say "Grep test:";
# Original bash: echo "alpha beta gamma" | grep beta
my $output_0 = q{};
say $output_0;
say "---";
# Original bash: echo "alpha beta gamma" | grep -o beta
my $output_1 = q{};
say $output_1;
say "done";

exit $main_exit_code;


--- Running generated Perl code ---
Exit code: exit status: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0138 seconds
Bash execution time:  0.0346 seconds
Perl is 2.51x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-Grep test:
-alpha beta gamma
----
-beta
-done


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
+Can't open perl script "__tmp_run.pl": No such file or directory


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(2)
