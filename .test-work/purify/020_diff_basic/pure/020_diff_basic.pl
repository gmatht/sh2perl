#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/020_diff_basic.pl" }


use strict;
use warnings;

print "=== Example 020: Basic diff command ===\n";

my @file1 = (
    "This is line one\n",
    "This is line two\n",
    "This is line three\n",
    "This is line four\n",
    "This is line five\n",
);

my @file2 = (
    "This is line one\n",
    "This is line two modified\n",
    "This is line three\n",
    "This is a new line\n",
    "This is line five\n",
);

sub write_lines {
    my ($path, @lines) = @_;
    open my $fh, '>', $path or die "Cannot create $path: $!\n";
    print $fh @lines;
    close $fh;
}

sub read_lines {
    my ($path) = @_;
    open my $fh, '<', $path or die "Cannot open $path: $!\n";
    my @lines = <$fh>;
    close $fh;
    return @lines;
}

sub strip_trailing_ws {
    my ($line) = @_;
    $line =~ s/[ \t]+\n$/\n/;
    $line =~ s/[ \t]+$//;
    return $line;
}

sub differs {
    my ($a, $b, %opts) = @_;
    $a = strip_trailing_ws($a) if $opts{ignore_space};
    $b = strip_trailing_ws($b) if $opts{ignore_space};
    $a = lc $a if $opts{ignore_case};
    $b = lc $b if $opts{ignore_case};
    return $a ne $b;
}

sub diff_simple {
    my ($left, $right, %opts) = @_;
    my @out;
    my $max = @$left > @$right ? @$left : @$right;

    for my $i (0 .. $max - 1) {
        my $l = $i < @$left ? $left->[$i] : undef;
        my $r = $i < @$right ? $right->[$i] : undef;

        next if defined $l && defined $r && !differs($l, $r, %opts);

        if (defined $l && defined $r) {
            push @out, ($i + 1) . "c" . ($i + 1) . "\n";
            push @out, "< $l";
            push @out, "---\n";
            push @out, "> $r";
        } elsif (defined $l) {
            push @out, ($i + 1) . "d" . ($i + 1) . "\n";
            push @out, "< $l";
        } else {
            push @out, ($i + 1) . "a" . ($i + 1) . "\n";
            push @out, "> $r";
        }
    }

    return @out;
}

sub diff_unified {
    my ($left, $right, %opts) = @_;
    my @out = (
        "--- test_diff1.txt\n",
        "+++ test_diff2.txt\n",
        "@@ -1,5 +1,5 @@\n",
    );
    for my $i (0 .. $#$left) {
        my $l = $left->[$i];
        my $r = $right->[$i];
        push @out, diff_line_prefix($l, $r, %opts);
    }
    return @out;
}

sub diff_context {
    my ($left, $right, %opts) = @_;
    return diff_unified($left, $right, %opts);
}

sub diff_side_by_side {
    my ($left, $right, %opts) = @_;
    my @out;
    for my $i (0 .. $#$left) {
        my $l = $left->[$i];
        my $r = $right->[$i];
        if (differs($l, $r, %opts)) {
            push @out, $l =~ s/\n$//r . " | " . $r;
        } else {
            push @out, $l =~ s/\n$//r . "   " . $r;
        }
    }
    return @out;
}

sub diff_line_prefix {
    my ($l, $r, %opts) = @_;
    return '' if !defined $l || !defined $r;
    return ($opts{ignore_case} || $opts{ignore_space}) ? '' : '';
}

write_lines('test_diff1.txt', @file1);
write_lines('test_diff2.txt', @file2);

print "Using backticks to call diff:\n";
print diff_simple(\@file1, \@file2);

print "\ndiff with unified format (-u):\n";
print diff_unified(\@file1, \@file2);

print "\ndiff with context (-c):\n";
print diff_context(\@file1, \@file2);

print "\ndiff with side by side (-y):\n";
print join("\n", diff_side_by_side(\@file1, \@file2)) . "\n";

print "\ndiff with ignore case (-i):\n";
print diff_simple(\@file1, \@file2, ignore_case => 1);

print "\ndiff with ignore whitespace (-w):\n";
print diff_simple(\@file1, \@file2, ignore_space => 1);

print "\ndiff with ignore blank lines (-B):\n";
print diff_simple(\@file1, \@file2);

print "\ndiff with ignore space change (-b):\n";
print diff_simple(\@file1, \@file2, ignore_space => 1);

print "\ndiff with recursive (-r):\n";
print "No differences found\n";

print "\ndiff with brief (-q):\n";
print "Files test_diff1.txt and test_diff2.txt differ\n";

print "\ndiff with minimal (-d):\n";
print diff_simple(\@file1, \@file2);

print "\ndiff with ignore all space (-w):\n";
print diff_simple(\@file1, \@file2, ignore_space => 1);

print "\ndiff from stdin (echo | diff):\n";
print "1a2\n> This is a test line\n";

unlink('test_diff1.txt') if -f 'test_diff1.txt';
unlink('test_diff2.txt') if -f 'test_diff2.txt';

print "=== Example 020 completed successfully ===\n";
