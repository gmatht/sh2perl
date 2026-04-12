#!/usr/bin/perl


use strict;
use warnings;

print "=== Example 016: Basic sort command ===\n";

my @words = qw(Charlie Alice Bob Diana Eve);
my @numbers = qw(25 10 5 30 15);
my @csv_rows = (
    [qw(Alice 25 95.5)],
    [qw(Bob 30 87.2)],
    [qw(Charlie 20 92.8)],
);
my @multi_rows = (
    [qw(Alice 25)],
    [qw(Bob 30)],
    [qw(Alice 20)],
);
my @human = qw(1K 2M 500 1G);
my @versions = qw(v1.2 v1.10 v1.1);

sub print_lines {
    print join('', map { "$_\n" } @_);
}

sub print_rows_csv {
    my ($rows) = @_;
    print join('', map { join(',', @$_) . "\n" } @$rows);
}

sub print_rows_space {
    my ($rows) = @_;
    print join('', map { join(' ', @$_) . "\n" } @$rows);
}

sub numeric_value {
    my ($text) = @_;
    return $1 * 1024 * 1024 * 1024 if $text =~ /^(\d+)G$/i;
    return $1 * 1024 * 1024 if $text =~ /^(\d+)M$/i;
    return $1 * 1024 if $text =~ /^(\d+)K$/i;
    return 0 + $text;
}

sub version_cmp {
    my ($left, $right) = @_;
    $left =~ s/^v//;
    $right =~ s/^v//;
    my @left = split /\./, $left;
    my @right = split /\./, $right;
    my $count = @left > @right ? @left : @right;
    for my $i (0 .. $count - 1) {
        my $l = $left[$i] // 0;
        my $r = $right[$i] // 0;
        return $l <=> $r if $l != $r;
    }
    return 0;
}

print "Using backticks to call sort (alphabetical):\n";
print_lines(sort @words);

print "\nsort with reverse (-r):\n";
print_lines(sort { $b cmp $a } @words);

print "\nsort with numeric (-n):\n";
print_lines(sort { $a <=> $b } @numbers);

print "\nsort with unique (-u):\n";
my %seen;
print_lines(grep { !$seen{$_}++ } sort @words);

print "\nsort with case insensitive (-f):\n";
print_lines(sort { lc($a) cmp lc($b) } @words);

print "\nsort with field separator (sort by second field):\n";
my @by_second = sort { $a->[1] <=> $b->[1] } @csv_rows;
print_rows_csv(\@by_second);

print "\nsort with multiple keys:\n";
my @sorted_multi = sort {
    $a->[0] cmp $b->[0] || $a->[1] <=> $b->[1]
} @multi_rows;
print_rows_space(\@sorted_multi);

print "\nsort with human readable (-h):\n";
print_lines(sort { numeric_value($a) <=> numeric_value($b) } @human);

print "\nsort with version (-V):\n";
print_lines(sort { version_cmp($a, $b) } @versions);

print "\nsort with reverse again (-r):\n";
print_lines(sort { $b cmp $a } @words);

print "\nsort from stdin (echo | sort):\n";
print_lines(sort qw(Zebra Apple Banana));

print "\nsort with merge (-m):\n";
my @sorted_once = sort @words;
print_lines(@sorted_once, @sorted_once);

print "\nsort with check (-c):\n";
my @check = @words;
my $sorted = 1;
for my $i (1 .. $#check) {
    if ($check[$i - 1] gt $check[$i]) {
        $sorted = 0;
        last;
    }
}
print $sorted ? "input is sorted\n" : "input is not sorted\n";

print "=== Example 016 completed successfully ===\n";
