#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'tests/tier1/native_capture_cmdsub.sh';

use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

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
say "x=" . __sh_str(($x));
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
    say "y=" . __sh_str(($y));
    $? = 0 << 8;
    $? = 0 << 8;
}
show("from-argument");
$z = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say $greeting;
    $? = 0 << 8;
    $? = 0 << 8;
    say "inside=" . (do { my $__sh_cap; $__sh_cap = q{sh2perl}; $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap });
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "z=" . __sh_str(($z));
$? = 0 << 8;
$n = do { my $__cap = ''; { local *STDOUT; open(STDOUT, '>', \$__cap) or die "capture: $!\n";     say do { my $__sh_cap; $__sh_cap = q{c}; $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
    $? = 0 << 8;
;; } $__cap =~ s/\n+\z//; $__cap };
say "n=" . __sh_str(($n));
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
