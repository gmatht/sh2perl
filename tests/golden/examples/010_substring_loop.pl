#!/usr/bin/env perl
use strict;
use warnings;
my $s;
my $i;
my $len;
my $ch;
my %s;

$s = "HELLOWORLD";
$i = "0";
$len = "10";
while (int($i) < int($len)) {
    $ch = substr($s, $i, 1);
printf("%s", $ch);
        $i = int(($i + 1));
}
print "\n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
