#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
my $output         = q{};
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
my $output_1 = qx{: 'Complex command not supported in bash string generation' | sort};
chomp $output_1;
print $output_1, "\n";

