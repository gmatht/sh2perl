#!/usr/bin/env perl
use strict;
use warnings;
my $y;
my $z;

$y = "world";
$z = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print(join(' ', grep { defined($_) && length($_) } split(/\s+/, $y)), "\n");
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
print($z, "\n");
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
