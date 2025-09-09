Executing shell command: perl
==================================================
Generated Perl code:
#!/usr/bin/env perl
use strict;
use warnings;

perl_output_0 = `perl `;
print $perl_output_0;


--- Running generated Perl code ---
Exit code: exit code: 255

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0409 seconds
Bash execution time:  0.0793 seconds
Perl is 1.94x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
+Can't modify constant item in scalar assignment at __tmp_direct_exec.pl line 5, near "`perl `;"
+Global symbol "$perl_output_0" requires explicit package name (did you forget to declare "my $perl_output_0"?) at __tmp_direct_exec.pl line 6.
+Bareword "perl_output_0" not allowed while "strict subs" in use at __tmp_direct_exec.pl line 5.
+Execution of __tmp_direct_exec.pl aborted due to compilation errors.


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(255)
==================================================
