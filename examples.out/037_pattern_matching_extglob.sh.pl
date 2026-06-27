#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["f1", "f2"]
my $f1 = 0;
my $f2 = 0;

# set -euo
# set pipefail
print("== extglob ==\n");
# extglob option enabled
$f1 = "file.js";
$ENV{f1} = $f1;
$f2 = "thing.min.js";
$ENV{f2} = $f2;
my $pipeline_result_1 = (($f1 =~ /^(?!.*\.min\.js).*\.js$/)) && (print("f1-ok\n"));
my $pipeline_result_2 = (($f2 =~ /^(?!.*\.min\.js).*\.js$/)) || (print("f2-filtered\n"));