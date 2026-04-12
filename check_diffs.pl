#!/usr/bin/perl
use strict;
use warnings;

open my $fh1, '<', 'examples.out/000__03_file_manipulation_commands.sh.pl' or die $!;
open my $fh2, '<', 'tidied_code.txt' or die $!;

my @orig = <$fh1>;
my @tidy = <$fh2>;
close $fh1;
close $fh2;

# Normalize line endings
s/\r\n/\n/g for @orig;
s/\r\n/\n/g for @tidy;

my $max = $#orig > $#tidy ? $#orig : $#tidy;
my $diff_count = 0;

for my $i (0..$max) {
    my $orig_line = $orig[$i] // "";
    my $tidy_line = $tidy[$i] // "";
    
    if ($orig_line ne $tidy_line) {
        $diff_count++;
        print "Line " . ($i+1) . ":\n";
        print "  ORIG: [" . $orig_line . "]\n";
        print "  TIDY: [" . $tidy_line . "]\n";
        print "\n";
        last if $diff_count > 20;
    }
}

print "Total differences found: $diff_count\n";

