#!/usr/bin/env perl
use strict;
use warnings;


sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

do { my $__ok = 1; for my $__d ('status_dir') { unless (mkdir($__d)) { $__ok = 0; print STDERR "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; };
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, __sh_str(($? >> 8)))), "\n");
$? = 0 << 8;

exit $? >> 8;
