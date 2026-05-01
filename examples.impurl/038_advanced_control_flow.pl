#!/usr/bin/perl

# Example 038: Advanced control flow demonstrating external commands in small parts

use strict;
use warnings;

print "=== Example 038: Advanced control flow ===\n";

my @lines = (
    'Line 1: This is a test',
    'Line 2: Another test line',
    'Line 3: Third test line',
    'Line 4: Fourth test line',
    'Line 5: Fifth test line',
);

print "Advanced if-else with builtins:\n";
my $file_size = 123;
if ($file_size > 100) {
    print "File is large ($file_size bytes), compressing:\n";
    print "(demo) gzip -c file > file.gz\n";
} else {
    print "File is medium or small ($file_size bytes), listing lines:\n";
    print join("\n", @lines), "\n";
}

print "\nNested loops with builtins:\n";
for my $i (1..3) {
    print "Outer loop iteration $i:\n";
    for my $j (1..2) {
        print "  Inner loop iteration $j: $i-$j\n";
    }
}

print "\nSwitch-like statement with a small grep example:\n";
print "Lines matching 'test':\n";
print join("\n", grep { /test/ } @lines), "\n";

print "\nFunction with builtins (returns counts):\n";
sub process_file_with_builtins { return scalar(@lines) }
print "Lines: ", process_file_with_builtins(), "\n";

print "\nLoop with break and continue:\n";
for my $i (1..5) {
    print "Iteration $i...\n";
    next if $i == 3;
    last if $i == 4;
    print "Processed $i\n";
}

print "\nRecursive function with builtins (depth 1..3):\n";
sub recursive_process { my ($d,$m)=@_; return if $d>$m; print "  " x $d . "Depth $d\n"; recursive_process($d+1,$m) }
recursive_process(1,3);

print "\nException handling with builtins:\n";
eval { die "Command failed with exit code 127" };
print "Falling back to safe operation\n" if $@;

print "\n=== Example 038 completed ===\n";

# Small deterministic external-command demo
print "\nExternal commands demo (deterministic):\n";
my $tmp_adv = 'adv_lines.txt';
open my $advfh, '>', $tmp_adv or die "cannot write $tmp_adv: $!";
print $advfh join("\n", @lines), "\n";
close $advfh;

print "\ngrep 'test' $tmp_adv\n";
print qx/grep 'test' $tmp_adv/;

print "\ngrep -n 'test' $tmp_adv\n";
print qx/grep -n 'test' $tmp_adv/;

print "\nwc -l $tmp_adv\n";
print qx/wc -l $tmp_adv/;

unlink $tmp_adv or warn "could not remove $tmp_adv: $!";
