#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '006_misc.sh';
print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print "inside-subshell\n";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_132 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'alpha beta' | grep beta}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_132, "\n");
print("exit: " . ($? >> 8), "\n");

exit $main_exit_code;
