#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print "inside-subshell\
";
    q{};
};
print "== Simple pipeline ==\n";
# Original bash: echo "alpha beta" | grep beta
my $output_0 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'alpha beta' | grep beta}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_0, "\n";
print "exit: " . ($? >> 8), "\n";

exit $main_exit_code;

