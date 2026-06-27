#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["word"]
my $word = 0;

# set -euo
# set pipefail
print("== nocasematch ==\n");
# nocasematch option enabled
$word = "Foo";
$ENV{word} = $word;
my $pipeline_result_1 = (($word =~ /^^foo$$/i)) && (print("ci-match\n"));