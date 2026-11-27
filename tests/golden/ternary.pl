#!/usr/bin/env perl
use strict;
use warnings;
my $x;
my $y;

$x = int(((1 > 0) ? 10 : 20));
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, $x)), "\n");
$y = int((0 ? 1 : 2));
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, $y)), "\n");
$? = 0 << 8;

exit $? >> 8;
