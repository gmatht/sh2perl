#!/usr/bin/env perl

use strict;
use warnings;
use File::Find;

# Pipeline examples
# Note: Perl doesn't have direct pipeline syntax like shell, but we can simulate it

# ls | grep "\.txt$" | wc -l
my $txt_count = 0;
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next if $file =~ /^\.\.?$/;  # Skip . and ..
    $txt_count++ if $file =~ /\.txt$/;
}
closedir($dh);
print "$txt_count\n";

print "\n";

# cat file.txt | sort | uniq -c | sort -nr
if (-f "file.txt") {
    my %count;
    open(my $fh, '<', "file.txt") or die "Cannot open file.txt: $!\n";
    while (my $line = <$fh>) {
        chomp $line;
        $count{$line}++;
    }
    close($fh);
    
    # Sort by count (descending), then by value (ascending)
    for my $line (sort { $count{$b} <=> $count{$a} || $a cmp $b } keys %count) {
        print "$count{$line} $line\n";
    }
}
print "\n";

# find . -name "*.sh" | xargs grep -l "function" | tr -d "\\\\/"
# This is complex to replicate exactly, but here's a simplified version:
my @sh_files;
find(sub { push @sh_files, $File::Find::name if /\.sh$/ }, '.');
for my $file (@sh_files) {
    if (-f $file) {
        open(my $fh, '<', $file) or next;
        my $content = do { local $/; <$fh> };
        close($fh);
        if ($content =~ /function/) {
            my $clean_name = $file;
            $clean_name =~ s/[\\\/]//g;
            print "$clean_name\n";
        }
    }
}
print "\n";

# cat file.txt | tr 'a' 'b' | grep 'hello'
if (-f "file.txt") {
    open(my $fh, '<', "file.txt") or die "Cannot open file.txt: $!\n";
    while (my $line = <$fh>) {
        $line =~ tr/a/b/;
        print $line if $line =~ /hello/;
    }
    close($fh);
}
print "\n";

# cat file.txt | sort | grep 'hello'
if (-f "file.txt") {
    my @lines;
    open(my $fh, '<', "file.txt") or die "Cannot open file.txt: $!\n";
    while (my $line = <$fh>) {
        chomp $line;
        push @lines, $line;
    }
    close($fh);
    
    @lines = sort @lines;
    for my $line (@lines) {
        print "$line\n" if $line =~ /hello/;
    }
}
