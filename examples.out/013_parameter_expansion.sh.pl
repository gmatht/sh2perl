#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 5 variables: ["name", "path", "s2", "var", "maybe"]
my $name = 0;
my $path = 0;
my $s2 = 0;
my $var = 0;
my $maybe = 0;

# set -euo
# set pipefail
print("== Case modification in parameter expansion ==\n");
$name = "world";
$ENV{name} = $name;
print(uc($name) . "\n");
print(lc($name) . "\n");
print(ucfirst($name) . "\n");
print("== Advanced parameter expansion ==\n");
$path = "/tmp/file.txt";
$ENV{path} = $path;
print(basename($path) . "\n");
print(dirname($path) . "\n");
$s2 = "abba";
$ENV{s2} = $s2;
print(do { my $temp = $s2; $temp =~ s/b/X/g; $temp } . "\n");
print("== More parameter expansion ==\n");
$var = "hello world";
$ENV{var} = $var;
print(do { my $temp = $var; $temp =~ s/^hello//; $temp } . "\n");
print(do { my $temp = $var; $temp =~ s/world$//; $temp } . "\n");
print(do { my $temp = $var; $temp =~ s/o/0/g; $temp } . "\n");
print("== Default values ==\n");
undef $maybe;
print((defined($ENV{maybe}) ? $ENV{maybe} : 'default') . "\n");
print(($ENV{maybe} //= 'default') . "\n");
print((defined($ENV{maybe}) ? $ENV{maybe} : die('error')) . "\n");