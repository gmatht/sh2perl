#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== Practical examples ==\n");
open(my $fh, '>', 'file_001.txt') or die "Cannot create file: $!\n";
close($fh);
open(my $fh, '>', 'file_002.txt') or die "Cannot create file: $!\n";
close($fh);
open(my $fh, '>', 'file_003.txt') or die "Cannot create file: $!\n";
close($fh);
open(my $fh, '>', 'file_004.txt') or die "Cannot create file: $!\n";
close($fh);
open(my $fh, '>', 'file_005.txt') or die "Cannot create file: $!\n";
close($fh);
opendir(my $dh_1, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh_1)) {
if ($file =~ /^^file\_.*\.txt$$/) {
print("$file\n");
}
}
closedir($dh_1);
opendir(my $dh_2, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh_2)) {
if ($file =~ /^^file\_.*\.txt$$/) {
unlink($file) or die "Cannot remove file: $!\n";
}
}
closedir($dh_2);