#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

my $greeting;
my $x;

$greeting = "hi";
$x = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say $greeting;
    $? = 0 << 8;
    $? = 0 << 8;
    print "second\n";
    $? = 0 << 8;
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say $x;
$? = 0 << 8;
printf("%s\n", $x);
$? = 0 << 8;

exit $? >> 8;
