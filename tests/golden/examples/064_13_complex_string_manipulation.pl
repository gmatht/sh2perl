#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/064_13_complex_string_manipulation.sh';

use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

my $filename;
my $basename;
my $extension;
my $uppercase;
my $lowercase;

$filename = "my_file.txt";
$basename = ((__sh_str($filename) =~ s{\.[^.]*$}{}r));
$extension = ((__sh_str($filename) =~ s{^.*\.}{}r));
$uppercase = "MY_FILE.TXT";
$lowercase = "my_file.txt";
say "Basename: " . __sh_str(($basename));
$? = 0 << 8;
say "Extension: " . __sh_str(($extension));
$? = 0 << 8;
say "Uppercase: " . __sh_str(($uppercase));
$? = 0 << 8;
say "Lowercase: " . __sh_str(($lowercase));
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;
