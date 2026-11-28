#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/077_backslash_continuation.sh';


use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

do { my $__t = 'hello' . "\n"; $__t =~ tr/a-z/A-Z/; print $__t; $? = 0 << 8; };
say "exit: " . __sh_str((($? >> 8)));
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
