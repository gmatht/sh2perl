#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/param-expand-default-operator.sh';
my $BASH_VERSION;

$ENV{BASH_VERSION} //= do { my $v = `bash -c 'echo \$BASH_VERSION'`; chomp $v; $v };
our $CHILD_ERROR = 0;

$CHILD_ERROR = (length((defined($ENV{ZSH_VERSION}) ? $ENV{ZSH_VERSION} : ""))) ? 0 : 1;
if (length((defined($ENV{ZSH_VERSION}) ? $ENV{ZSH_VERSION} : ""))) {
    print "zsh\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} elsif (length((defined($ENV{BASH_VERSION}) ? $ENV{BASH_VERSION} : ""))) {
    print "bash\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}

exit $? >> 8;
