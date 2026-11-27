#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

my $filename;
my $basename;
my $extension;
my $uppercase;
my $lowercase;

$filename = "my_file.txt";
$basename = (($filename =~ s{\.[^.]*$}{}r));
$extension = (($filename =~ s{^.*\.}{}r));
$uppercase = "MY_FILE.TXT";
$lowercase = "my_file.txt";
say "Basename: $basename";
$? = 0 << 8;
say "Extension: $extension";
$? = 0 << 8;
say "Uppercase: $uppercase";
$? = 0 << 8;
say "Lowercase: $lowercase";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
