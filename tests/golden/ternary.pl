#!/usr/bin/env perl
use strict;
use warnings;

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

my $x;
my $y;

$x = int(((1 > 0) ? 10 : 20));
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, __sh_str($x))), "\n");
$y = int((0 ? 1 : 2));
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, __sh_str($y))), "\n");
$? = 0 << 8;

exit $? >> 8;
