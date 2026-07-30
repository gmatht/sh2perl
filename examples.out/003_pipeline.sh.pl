#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '003_pipeline.sh';
# Original bash: ls | grep "\.txt$" | wc -l
my $output_0 = do { open(my $__fh, '-|', 'bash', '-c', 'ls | grep "\\\\.txt\\$" | wc -l') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_0, "\n");
print "\n";
# Original bash: cat file.txt | sort | uniq -c | sort -nr
my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', 'cat file.txt | sort | uniq -c | sort -nr') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_1, "\n");
print "\n";
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
my $output_2 = do { open(my $__fh, '-|', 'bash', '-c', q{find . -name '*.sh' | xargs grep -l function | tr -d "\\\\/"}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_2, "\n");
print "\n";
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
my $output_3 = do { open(my $__fh, '-|', 'bash', '-c', 'cat file.txt | tr a b | grep hello') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_3, "\n");
print "\n";
my $output_4 = do { open(my $__fh, '-|', 'bash', '-c', 'cat file.txt | sort | grep hello') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_4, "\n");

exit $main_exit_code;
