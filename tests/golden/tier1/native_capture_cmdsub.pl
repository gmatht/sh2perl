#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

our @__sh_args;
our $__sh_args_n;
@__sh_args = @ARGV;
$__sh_args_n = scalar(@__sh_args);
my $__argc = @ARGV;
my $greeting;
my $x;
my $y;
my $z;
my $n;

$greeting = ((defined($__sh_args[0]) && length($__sh_args[0])) ? $__sh_args[0] : "hi");
$x = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say $greeting;
    $? = 0 << 8;
    $? = 0 << 8;
    print "second\n";
    $? = 0 << 8;
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "x=$x";
$? = 0 << 8;
sub show {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    $y = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say $__sh_args[0];
    $? = 0 << 8;
    $? = 0 << 8;
    print "tail-part\n";
    $? = 0 << 8;
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
    say "y=$y";
    $? = 0 << 8;
    $? = 0 << 8;
}
show("from-argument");
$z = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say $greeting;
    $? = 0 << 8;
    $? = 0 << 8;
    print("inside=" . (do { my $__sh_cap; $__sh_cap = q{sh2perl}; $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }), "\n");
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "z=$z";
$? = 0 << 8;
$n = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     print(do { my $__sh_cap; $__sh_cap = q{c}; $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap }, "\n");
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "n=$n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
