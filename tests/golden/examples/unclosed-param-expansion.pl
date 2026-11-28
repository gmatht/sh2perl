#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/unclosed-param-expansion.sh';

use feature 'say';

say do { my $__t = $ENV{HOME}; my $__r = $__t; for (my $__i = length($ENV{HOME}); $__i >= 0; $__i--) { if (substr($__t, $__i) =~ /\A(?:\/)\z/) { $__r = substr($__t, 0, $__i); last } } $__r };
$? = 0 << 8;

exit $? >> 8;
