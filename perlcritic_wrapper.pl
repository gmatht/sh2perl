#!/usr/bin/env perl
# Wrapper script for Perl::Critic used by debashc tests.
# Usage: perl perlcritic_wrapper.pl [--profile <config_file>] <perl_file>
# Runs perlcritic on the given file with the specified profile.

use strict;
use warnings;

my $profile;
my @extra_args;

while (@ARGV && $ARGV[0] =~ /^--/) {
    my $arg = shift @ARGV;
    if ($arg eq '--profile' && @ARGV) {
        $profile = shift @ARGV;
    } else {
        push @extra_args, $arg;
    }
}

my $file = shift @ARGV
    or die "Usage: perlcritic_wrapper.pl [--profile <file>] <perl_file>\n";

my @cmd = ('perlcritic');
if (defined $profile) {
    push @cmd, '--profile', $profile;
}
push @cmd, @extra_args;
push @cmd, $file;

system(@cmd);
exit($? >> 8);
