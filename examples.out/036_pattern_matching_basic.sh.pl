#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["s"]
my $s = 0;

# set -euo
# set pipefail
print("== [[ pattern and regex ]]\n");
$s = "file.txt";
$ENV{s} = $s;
my $pipeline_result_1 = (($s =~ /^^.*\.txt$$/)) && (print("pattern-match\n"));
my $pipeline_result_2 = (($s =~ /^file\.[a-z]+$/)) && (print("regex-match\n"));