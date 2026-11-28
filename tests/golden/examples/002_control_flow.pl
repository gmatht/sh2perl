#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/002_control_flow.sh';


use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

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

our @__sh_args;
our $__sh_args_n;
@__sh_args = @ARGV;
$__sh_args_n = scalar(@__sh_args);
my $__argc = @ARGV;
my $i;

if (-f 'file.txt') {
    print "File exists\n";
    $? = 0 << 8;
    } else {
    print "File does not exist\n";
    $? = 0 << 8;
    }
for my $__i (("1", "2", "3", "4", "5")) {
    $i = $__i;
    say "Number: " . __sh_str(($i));
    $? = 0 << 8;
    }
while (int(__sh_num($i)) < int(10)) {
    say "Counter: " . __sh_str(($i));
    $? = 0 << 8;
        $i = int((__sh_num($i) + 1));
}
sub greet {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    return "Hello, " . __sh_str(($__sh_args[0])) . "!";
}
say greet("World");
$? = 0 << 8;

exit $? >> 8;
