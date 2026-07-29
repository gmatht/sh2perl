#!/usr/bin/env perl
use strict;
use warnings;
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $word;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== nocasematch ==\n";
# nocasematch option enabled
$word = "Foo";
if ($word =~ /^foo$/mi) {
        print "ci-match\
";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;

