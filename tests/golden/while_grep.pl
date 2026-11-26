#!/usr/bin/env perl
use strict;
use warnings;

while (((sub { open(my $__gh, '<', '/etc/hostname') or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\n/, $__gc)) { $__gn++ if index($__gl, 'x') >= 0; } $__gn })->() > 0)) {
    print "x\n";
    $? = 0 << 8;
    $? = 0 << 8;
    last;
}

exit $? >> 8;
