#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

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
    say "Number: $i";
    $? = 0 << 8;
    }
while (int($i) < int(10)) {
    say "Counter: $i";
    $? = 0 << 8;
        $i = int(($i + 1));
}
sub greet {
    local @__sh_args = @_;
    my $__argc = @__sh_args;
    return "Hello, " . ($__sh_args[0]) . "!";
}
say greet("World");
$? = 0 << 8;

exit $? >> 8;
