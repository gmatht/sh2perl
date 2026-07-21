#!/usr/bin/env perl
use strict;
use warnings;

# Scan generated Perl files for qx{} and system() calls with known builtins.
# Exit code = number of violations found.

my @builtins = qw(
    find ls grep sed awk sort uniq head tail cat echo printf
    touch mkdir rmdir rm cp mv chmod chown ln basename dirname
    date sleep wc kill ps cd pwd perl
);

my $violations = 0;

for my $file (@ARGV ? @ARGV : glob('examples.out/*.pl')) {
    next unless -f $file;
    open my $fh, '<', $file or next;
    my $code = do { local $/; <$fh> };
    close $fh;

    my $basename = (split '/', $file)[-1];
    $basename =~ s/\.pl$//;

    # Pattern 1: direct qx{builtin ...}
    for my $b (@builtins) {
        if ($code =~ /qx\{[^}]*\b\Q$b\E\b/) {
            print "QX VIOLATION: $basename has qx{} call with builtin '$b'\n";
            $violations++;
            last;
        }
    }

    # Pattern 2: qx{\$var} where var was assigned a string containing a builtin
    while ($code =~ /qx\{(\$\w+)\}/g) {
        my $var = $1;
        my $before = substr($code, 0, pos($code));
        for my $b (@builtins) {
            if ($before =~ /my\s+\Q$var\E\s*=\s*(?:q\{[^}]*\b\Q$b\E\b|"[^"]*\b\Q$b\E\b|'[^']*\b\Q$b\E\b)/s) {
                print "QX VIOLATION: $basename uses qx{$var} where $var contains builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }

    # Pattern 3: system('builtin') or system("builtin")
    for my $b (@builtins) {
        if ($code =~ /system\s*['"]\s*\Q$b\E\b/) {
            print "SYSTEM VIOLATION: $basename has system() call with builtin '$b'\n";
            $violations++;
            last;
        }
    }
}

exit $violations;
