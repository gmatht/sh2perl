#!/usr/bin/env perl
use strict;
use warnings;
my $x;

$x = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $__sh_cap }) }); $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
print($x, "\n");
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
