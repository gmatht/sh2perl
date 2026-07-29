#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success     = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '003_pipeline.sh';
# Original bash: ls | grep "\.txt$" | wc -l
my $output_127 = qx{command ls | grep "\\.txt\$" | wc -l};
chomp $output_127;
print $output_127, "\n";
print "\n";
# Original bash: cat file.txt | sort | uniq -c | sort -nr
my $output_128 = qx{command cat file.txt | sort | uniq -c | sort -nr};
chomp $output_128;
print $output_128, "\n";
print "\n";
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
my $output_129 = qx{command find . -name '*.sh' | xargs grep -l function | tr -d "\\\\/"};
chomp $output_129;
print $output_129, "\n";
print "\n";
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
my $output_130 = qx{command cat file.txt | tr a b | grep hello};
chomp $output_130;
print $output_130, "\n";
print "\n";
my $output_131 = qx{command cat file.txt | sort | grep hello};
chomp $output_131;
print $output_131, "\n";

exit $main_exit_code;
