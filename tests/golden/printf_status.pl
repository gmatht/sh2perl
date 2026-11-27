#!/usr/bin/env perl
use strict;
use warnings;

my $__sh2_pf_bad0 = 0;
printf("%d\n", (do {
    my ($__pf_s, $__pf_v, $__pf_bad, $__pf_msg) = ("zz", 0, 0, 'invalid number');
    my $__pf_t = $__pf_s;
    $__pf_t =~ s/\A[ \t\n]+//;
    if ($__pf_t =~ /\A([+-]?)0[xX]([0-9a-fA-F]*)(.*)\z/s) {
        $__pf_v = length($2) ? hex($2) : 0;
        $__pf_v = -$__pf_v if $1 eq '-';
        if (length($3) || !length($2)) { $__pf_bad = 1; $__pf_msg = 'invalid hex number'; }
    } elsif ($__pf_t =~ /\A([+-]?)0[bB]([01]*)(.*)\z/s) {
        $__pf_v = length($2) ? oct('0b' . $2) : 0;
        $__pf_v = -$__pf_v if $1 eq '-';
        $__pf_bad = 1 if length($3) || !length($2);
    } elsif ($__pf_t =~ /\A([+-]?)0([0-7]*)(.*)\z/s) {
        $__pf_v = length($2) ? oct('0' . $2) : 0;
        $__pf_v = -$__pf_v if $1 eq '-';
        if (length($3)) { $__pf_bad = 1; $__pf_msg = 'invalid octal number'; }
    } elsif ($__pf_t =~ /\A([+-]?)([0-9]+)(.*)\z/s) {
        $__pf_v = 0 + $2;
        $__pf_v = -$__pf_v if $1 eq '-';
        $__pf_bad = 1 if length($3);
    } else {
        $__pf_bad = 1;
    }
    if ($__pf_bad) { print STDERR "printf: $__pf_s: $__pf_msg\n"; $__sh2_pf_bad0 = 1; }
    $__pf_v
}));
$? = (($__sh2_pf_bad0 ? 1 : 0)) << 8;
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, ($? >> 8))), "\n");
$? = 0 << 8;

exit $? >> 8;
