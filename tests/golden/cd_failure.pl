#!/usr/bin/env perl
use strict;
use warnings;


sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

if (chdir("/no/such/dir")) {
    $? = 0 << 8;
} else {
    warn "cd: " . "/no/such/dir" . ": $!\n";
    $? = 1 << 8;
}
if ((0+__sh_str(($? >> 8))) == (0+'0')) {
    print "ok\n";
    $? = 0 << 8;
    $? = 0 << 8;
} else {
    print "bad\n";
    $? = 0 << 8;
    $? = 0 << 8;
}

exit $? >> 8;
