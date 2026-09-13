#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
my $main_exit_code = 0;
my $__set_e = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '009_arrays.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Indexed arrays ==\n";
my @arr = ('one', 'two', 'three');
print($arr[1], "\n");
print(scalar(@arr), "\n");
for my $x (@arr) {
printf('%s ', "${x}");
}
print "\n";
print "== Associative arrays ==\n";
my %map = ();
$map{"foo"} = 'bar';
$map{"answer"} = '42';
$map{"two"} = "1 + 1";
print($map{'foo'}, "\n");
print($map{'answer'}, "\n");
# Original bash: #!/usr/bin/env bash
my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', q{: 'Complex command not supported in bash string generation' | sort}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_1, "\n");

exit $main_exit_code;
