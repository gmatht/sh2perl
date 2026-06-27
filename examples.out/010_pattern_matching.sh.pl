#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 4 variables: ["s", "f1", "f2", "word"]
my $s = 0;
my $f1 = 0;
my $f2 = 0;
my $word = 0;

# set -euo
# set pipefail
print("== [[ pattern and regex ]]\n");
$s = "file.txt";
$ENV{s} = $s;
my $pipeline_result_1 = (($s =~ /^^.*\.txt$$/)) && (print("pattern-match\n"));
my $pipeline_result_2 = (($s =~ /^file\.[a-z]+$/)) && (print("regex-match\n"));
print("== extglob ==\n");
# extglob option enabled
$f1 = "file.js";
$ENV{f1} = $f1;
$f2 = "thing.min.js";
$ENV{f2} = $f2;
my $pipeline_result_3 = (($f1 =~ /^(?!.*\.min\.js).*\.js$/)) && (print("f1-ok\n"));
my $pipeline_result_4 = (($f2 =~ /^(?!.*\.min\.js).*\.js$/)) || (print("f2-filtered\n"));
print("== nocasematch ==\n");
# nocasematch option enabled
$word = "Foo";
$ENV{word} = $word;
my $pipeline_result_5 = (($word =~ /foo/i)) && (print("ci-match\n"));