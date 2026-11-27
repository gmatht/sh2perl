#!/usr/bin/env perl
use strict;
use warnings;
my $greeting;
my $x;

$greeting = "hi";
$x = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print($greeting, "\n");
    $? = 0 << 8;
    $? = 0 << 8;
    print "second\n";
    $? = 0 << 8;
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
print($x, "\n");
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
