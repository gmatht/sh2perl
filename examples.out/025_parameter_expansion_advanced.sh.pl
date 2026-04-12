#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/file.txt";
print basename($path) . "\n";
print dirname($path) . "\n";
my $s2;
$s2 = "abba";
print $s2 =~ s/b/X/grs, "\n";
