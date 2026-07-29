#!/usr/bin/env perl
use strict;
use warnings;
use FindBin;

my $examples_dir = "$FindBin::RealBin/examples";
my $out_dir      = "$FindBin::RealBin/examples.out";

my @sh_files = sort glob("$examples_dir/*.sh");
print "Checking " . scalar(@sh_files) . " examples for determinism...\n";

my $bash_ok = 0;
my $bash_bad = 0;
my $perl_ok = 0;
my $perl_bad = 0;

for my $sh_file (@sh_files) {
    my $base = (split '/', $sh_file)[-1];
    $base =~ s/\.sh$//;
    my $pl_file = "$out_dir/${base}.sh.pl";

    # Bash determinism
    my $sh1 = `bash "$sh_file" 2>/dev/null`;
    my $sh2 = `bash "$sh_file" 2>/dev/null`;
    if ($sh1 eq $sh2) {
        $bash_ok++;
    } else {
        print "NON-DETERMINISTIC (bash): $base\n";
        $bash_bad++;
    }

    # Perl determinism
    if (-f $pl_file) {
        my $pl1 = `perl "$pl_file" 2>/dev/null`;
        my $pl2 = `perl "$pl_file" 2>/dev/null`;
        if ($pl1 eq $pl2) {
            $perl_ok++;
        } else {
            print "NON-DETERMINISTIC (perl): $base\n";
            $perl_bad++;
        }
    }
}

print "\nBash: $bash_ok deterministic, $bash_bad non-deterministic\n";
print "Perl: $perl_ok deterministic, $perl_bad non-deterministic\n";
exit ($bash_bad + $perl_bad);
