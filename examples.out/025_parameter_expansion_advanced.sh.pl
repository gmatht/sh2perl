#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use File::Basename;
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '025_parameter_expansion_advanced.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Advanced parameter expansion ==";
my $path = "/tmp/025_param_expansion_file.txt";
say basename(${path});
say dirname(${path});
my $s2 = "abba";
say $s2 =~ s/b/X/grs;
