#!/usr/bin/env perl
use strict;
use warnings;
our @__sh_args;
our $__sh_args_n;
@__sh_args = @ARGV;
$__sh_args_n = scalar(@__sh_args);
my $__argc = @ARGV;

sub f {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    my $x = "1";
    return $x;
}
print(f(), "\n");
$? = 0 << 8;

exit $? >> 8;
