#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
my $main_exit_code = 0;
my $__set_e = 0;
my $output = '';
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Indexed arrays ==\n";
my @arr = ('one', 'two', 'three');
print $arr[1], "\n";
print scalar(@arr), "\n";
for my $x (@arr) {
printf('%s ', "$x");
}
print "\n";
print "== Associative arrays ==\n";
my %map = ();
$map{"foo"} = 'bar';
$map{"answer"} = '42';
$map{"two"} = "1 + 1";
print $map{'foo'}, "\n";
print $map{'answer'}, "\n";
my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', q{: 'Complex command not supported in bash string generation' | sort}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_1, "\n";

exit $main_exit_code;

