#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
use IPC::Open3;
my $main_exit_code = 0;
my $__set_e = 0;
our $CHILD_ERROR;
$0 = '025_parameter_expansion_advanced.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/025_param_expansion_file.txt";
print(basename(${path}), "\n");
print(dirname(${path}), "\n");
my $s2 = "abba";
print($s2 =~ s/b/X/grs, "\n");
