#!/usr/bin/env perl
use strict;
use warnings;

sub __sh_num {
    # bash arithmetic, not Perl's numeric context: unset and empty are 0,
    # and so is anything that does not parse as a number (bash warns for the
    # last case, but the value is 0 either way — the surrounding expression
    # must not inherit undef).
    my $__v = shift();
    return 0 if !defined($__v) || $__v eq q{};
    no warnings qw(numeric);
    return 0 + $__v;
}

my $s;
my $i;
my $len;
my $ch;
my %s;

$s = "HELLOWORLD";
$i = "0";
$len = "10";
while (int(__sh_num($i)) < int(__sh_num($len))) {
    $ch = substr($s, $i, 1);
printf("%s", $ch);
        $i = int((__sh_num($i) + 1));
}
print "\n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
