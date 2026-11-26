#!/usr/bin/env perl
use strict;
use warnings;
my $x;

$x = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print "a\\nb\n";
    $? = 0 << 8;
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
print($x, "\n");
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
