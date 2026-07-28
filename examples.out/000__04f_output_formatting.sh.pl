#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use Carp;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04f_output_formatting.sh';
say "=== Output and Formatting Commands ===";
my $echo_result = "Hello from backticks";
say "Echo result: $echo_result";
my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");
say "Printf result: $printf_result";
my $tee_result = do { chomp(my $result_106 = qx{echo 'test output' | tee test_tee.txt}); $result_106; };
say "Tee result: $tee_result";
unlink('test_tee.txt');
say "=== Output and Formatting Commands Complete ===";
