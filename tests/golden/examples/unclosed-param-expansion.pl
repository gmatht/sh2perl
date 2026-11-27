#!/usr/bin/env perl
use strict;
use warnings;

print(do { my $__t = $ENV{HOME}; my $__r = $__t; for (my $__i = length($ENV{HOME}); $__i >= 0; $__i--) { if (substr($__t, $__i) =~ /\A(?:\/)\z/) { $__r = substr($__t, 0, $__i); last } } $__r }, "\n");
$? = 0 << 8;

exit $? >> 8;
