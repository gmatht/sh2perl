#!/usr/bin/env perl
use strict;
use warnings;

# Get all commits with their hash and date
my @commits = `git log --pretty=format:"%H %aI" --reverse`;

foreach my $commit_line (@commits) {
    chomp $commit_line;
    my ($hash, $date) = split /\s+/, $commit_line, 2;
    
    # Get the size of this commit (changes introduced)
    my $size_bytes = `git show --stat --format="" $hash | tail -1`;
    chomp $size_bytes;
    
    # Extract total size from stat output (e.g., " 3 files changed, 45 insertions(+), 12 deletions(-)")
    my $total_changes = 0;
    if ($size_bytes =~ /(\d+)\s+insertions?\(\+\)/) {
        $total_changes += $1;
    }
    if ($size_bytes =~ /(\d+)\s+deletions?\(-\)/) {
        $total_changes += $1;
    }
    
    # Convert to MB (approximate: assume 1 line = 80 bytes average)
    my $size_mb = sprintf("%.2f", ($total_changes * 80) / (1024 * 1024));
    
    # Format date to be more readable
    $date =~ s/T/ /;
    $date =~ s/\.\d+Z$//;
    
    print "$hash $date ${size_mb}MB\n";
}
