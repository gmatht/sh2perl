#!/usr/bin/env perl
use strict;
use warnings;
my $__argc = @ARGV;

sub f {
    local @ARGV = @_;
    my $__argc = @ARGV;
    my $x = "1";
    return $x;
}
print(f(), "\n");
$? = 0 << 8;

exit $? >> 8;
