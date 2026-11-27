#!/usr/bin/env perl
use strict;
use warnings;

$? = ((length('abc')) ? 0 : 1) << 8;
if ((0+($? >> 8)) == (0+'0')) {
    print "yes\n";
    $? = 0 << 8;
    } else {
    print "no\n";
    $? = 0 << 8;
    }
$? = ((!length('abc')) ? 0 : 1) << 8;
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, ($? >> 8))), "\n");
$? = 0 << 8;

exit $? >> 8;
