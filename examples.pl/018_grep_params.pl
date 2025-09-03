#!/usr/bin/env perl

use strict;
use warnings;

# Grep parameters and options examples
# Demonstrates various pattern matching parameters

print "== Basic grep parameters ==\n";
my $text = "text with pattern";
if ($text =~ /PATTERN/i) {
    print "$text\n";
}

my @lines = qw(line1 line2 line3);
for my $line (@lines) {
    print "$line\n" unless $line =~ /line2/;
}

@lines = qw(match no\ match match\ again);
my $count = 0;
for my $line (@lines) {
    $count++ if $line =~ /match/;
}
print "$count\n";

print "== Context parameters ==\n";
@lines = qw(line1 line2 TARGET line4 line5);
my $target_index = -1;
for my $i (0..$#lines) {
    if ($lines[$i] eq 'TARGET') {
        $target_index = $i;
        last;
    }
}

if ($target_index >= 0) {
    # After context (A 2)
    for my $i ($target_index..min($target_index + 2, $#lines)) {
        print "$lines[$i]\n";
    }
    
    # Before context (B 2)
    for my $i (max(0, $target_index - 2)..$target_index) {
        print "$lines[$i]\n";
    }
    
    # Both context (C 1)
    for my $i (max(0, $target_index - 1)..min($target_index + 1, $#lines)) {
        print "$lines[$i]\n";
    }
}

print "== File handling parameters ==\n";
open(my $fh, '>', "temp_file.txt") or die "Cannot create temp file: $!\n";
print $fh "content\n";
close($fh);

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
while (my $line = <$fh>) {
    if ($line =~ /content/) {
        print "temp_file.txt:$line";
    }
}
close($fh);

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
while (my $line = <$fh>) {
    if ($line =~ /content/) {
        print $line;
    }
}
close($fh);

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
my $has_content = 0;
while (my $line = <$fh>) {
    if ($line =~ /content/) {
        $has_content = 1;
        last;
    }
}
close($fh);
print "temp_file.txt\n" if $has_content;

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
my $has_nonexistent = 0;
while (my $line = <$fh>) {
    if ($line =~ /nonexistent/) {
        $has_nonexistent = 1;
        last;
    }
}
close($fh);
print "temp_file.txt\n" unless $has_nonexistent;

print "== Output formatting parameters ==\n";
$text = "text with pattern in it";
if ($text =~ /(pattern)/) {
    print "$1\n";
}

if ($text =~ /pattern/) {
    my $pos = $-[0];
    print "$pos:$text\n";
}

if ($text =~ /pattern/) {
    print "1:$text\n";  # Line number would be tracked in real implementation
}

print "== Recursive and include/exclude parameters ==\n";
mkdir("test_dir") unless -d "test_dir";
open($fh, '>', "test_dir/file1.txt") or die "Cannot create test file: $!\n";
print $fh "pattern here\n";
close($fh);

open($fh, '>', "test_dir/file2.txt") or die "Cannot create test file: $!\n";
print $fh "no pattern\n";
close($fh);

# Recursive search
find_and_grep("test_dir", "pattern");
find_and_grep("test_dir", "pattern", "*.txt");
find_and_grep("test_dir", "pattern", undef, "*.bak");

# Count matches
my $match_count = 0;
find_and_grep("test_dir", "pattern", "*.txt", undef, \$match_count);
print "$match_count\n";

print "== Advanced parameters ==\n";
@lines = qw(match1 match2 match3 match4);
$count = 0;
for my $line (@lines) {
    if ($line =~ /match/ && $count < 2) {
        print "$line\n";
        $count++;
    }
}

$text = "text with pattern in it";
if ($text =~ /pattern/) {
    print "found\n";
} else {
    print "not found\n";
}

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
$has_content = 0;
while (my $line = <$fh>) {
    if ($line =~ /pattern/) {
        $has_content = 1;
        last;
    }
}
close($fh);
if ($has_content) {
    print "temp_file.txt\0";
}

# Cleanup
unlink("temp_file.txt");
system("rm -rf test_dir");

sub find_and_grep {
    my ($dir, $pattern, $include, $exclude, $count_ref) = @_;
    opendir(my $dh, $dir) or return;
    while (my $file = readdir($dh)) {
        next if $file =~ /^\.\.?$/;
        my $full_path = "$dir/$file";
        if (-d $full_path) {
            find_and_grep($full_path, $pattern, $include, $exclude, $count_ref);
        } elsif (-f $full_path) {
            next if $include && $file !~ /$include/;
            next if $exclude && $file =~ /$exclude/;
            open(my $fh, '<', $full_path) or next;
            while (my $line = <$fh>) {
                if ($line =~ /$pattern/) {
                    if ($count_ref) {
                        $$count_ref++;
                    } else {
                        print "$full_path:$line";
                    }
                }
            }
            close($fh);
        }
    }
    closedir($dh);
}

sub min { $_[0] < $_[1] ? $_[0] : $_[1] }
sub max { $_[0] > $_[1] ? $_[0] : $_[1] }
