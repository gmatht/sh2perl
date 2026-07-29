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

# Original bash: ls | grep "\.txt$" | wc -l
my $output_0 = qx{command ls | grep "\\.txt\$" | wc -l};
chomp $output_0;
print $output_0, "\n";
print "\n";
# Original bash: cat file.txt | sort | uniq -c | sort -nr
my $output_1 = qx{command cat file.txt | sort | uniq -c | sort -nr};
chomp $output_1;
print $output_1, "\n";
print "\n";
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
my $output_2 = qx{command find . -name '*.sh' | xargs grep -l function | tr -d "\\\\/"};
chomp $output_2;
print $output_2, "\n";
print "\n";
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
my $output_3 = qx{command cat file.txt | tr a b | grep hello};
chomp $output_3;
print $output_3, "\n";
print "\n";
my $output_4 = qx{command cat file.txt | sort | grep hello};
chomp $output_4;
print $output_4, "\n";

exit $main_exit_code;

