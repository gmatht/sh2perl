#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== readarray/mapfile ==\n");
my $temp_file_input_1 = '/tmp/process_sub_input_7_1.tmp';
open(my $fh, '>', $temp_file_input_1) or die "Cannot create temp file: $!\n";
print $fh "x
y
";
close($fh);
my @lines = ();
open(my $fh_1, '<', '/tmp/process_sub_input_7_1.tmp') or die "Cannot open file: $!\n";
while (my $line = <$fh_1>) {
    chomp $line;
    push @lines, $line;
}
close($fh_1);
printf("%s ", join(" ", @lines));
print("\n");