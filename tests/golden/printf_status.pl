#!/usr/bin/env perl
use strict;
use warnings;

my $__sh2_pf_bad0 = 0;
printf("%d\n", (do {
    my ($__pf_s, $__pf_v, $__pf_bad, $__pf_msg) = ("zz", 0, 0, 'invalid number');
    my $__pf_t = $__pf_s;
    $__pf_t =~ s/\A[ \t\n]+//;
    if ($__pf_t =~ /\A(?<sg>[+-]?)0[xX](?<dg>[0-9a-fA-F]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? hex($+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest}) || !length($+{dg})) { $__pf_bad = 1; $__pf_msg = 'invalid hex number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0[bB](?<dg>[01]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0b' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest}) || !length($+{dg});
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)0(?<dg>[0-7]*)(?<rest>.*)\z/s) {
        $__pf_v = length($+{dg}) ? oct('0' . $+{dg}) : 0;
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        if (length($+{rest})) { $__pf_bad = 1; $__pf_msg = 'invalid octal number'; }
    } elsif ($__pf_t =~ /\A(?<sg>[+-]?)(?<dg>[0-9]+)(?<rest>.*)\z/s) {
        $__pf_v = 0 + $+{dg};
        $__pf_v = -$__pf_v if $+{sg} eq '-';
        $__pf_bad = 1 if length($+{rest});
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
