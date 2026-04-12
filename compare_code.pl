#!/usr/bin/perl
use strict;
use warnings;

# Read the actual generated code from a temp file if it exists
my $file = shift @ARGV || '__tmp_test_output.pl';
if (!-f $file) {
    print "File not found: $file\n";
    exit 1;
}

open my $fh, '<', $file or die "Cannot open $file: $!";
my $original = do { local $/; <$fh> };
close $fh;
$original =~ s/\r\n/\n/g;

# Get tidied version
my $tidied_out = `C:/Strawberry/perl/bin/perl.exe test_wrapper_minimal.pl "$file" 2>&1`;
if ($tidied_out =~ /Tidied:\s*(.+)/s) {
    my $tidied = $1;
    $tidied =~ s/\r\n/\n/g;
    
    if ($original ne $tidied) {
        my @o = split /\n/, $original;
        my @t = split /\n/, $tidied;
        print "Original lines: " . scalar(@o) . ", Tidied lines: " . scalar(@t) . "\n\n";
        for my $i (0..$#o) {
            if ($i <= $#t && $o[$i] ne $t[$i]) {
                print "Line " . ($i+1) . " differs:\n";
                print "  ORIG: [$o[$i]]\n";
                print "  TIDY: [" . ($t[$i] // "(missing)") . "]\n";
                print "\n";
            } elsif ($i > $#t) {
                print "Line " . ($i+1) . " only in original: [$o[$i]]\n";
            }
            last if $i > 50; # Limit output
        }
        if ($#o != $#t) {
            print "\nLine count differs: original has " . scalar(@o) . " lines, tidied has " . scalar(@t) . " lines\n";
        }
    } else {
        print "MATCH - code is tidy!\n";
    }
} else {
    print "Could not find Tidied section in output\n";
    print substr($tidied_out, 0, 500);
}














