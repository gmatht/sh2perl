#!/usr/bin/env perl

use strict;
use warnings;

# Advanced grep features and options
# Demonstrates specialized pattern matching capabilities

# Limit number of matches per file
my $text = "match1\nmatch2\nmatch3\nmatch4";
my $count = 0;
for my $line (split("\n", $text)) {
    if ($line =~ /match/ && $count < 2) {
        print "$line\n";
        $count++;
    }
}

# Show byte offset with output lines
$text = "text with pattern in it";
if ($text =~ /pattern/) {
    my $pos = $-[0];
    print "$pos:$text\n";
}

# Suppress filename prefix on output
open(my $fh, '>', "temp_file.txt") or die "Cannot create temp file: $!\n";
print $fh "content\n";
close($fh);

open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
while (my $line = <$fh>) {
    if ($line =~ /content/) {
        print $line;
    }
}
close($fh);

# Show filenames only (even with single file)
open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
while (my $line = <$fh>) {
    if ($line =~ /content/) {
        print "temp_file.txt:$line";
    }
}
close($fh);

# Null-terminated output (useful for xargs -0)
open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
my $has_pattern = 0;
while (my $line = <$fh>) {
    if ($line =~ /pattern/) {
        $has_pattern = 1;
        last;
    }
}
close($fh);
if ($has_pattern) {
    print "temp_file.txt\0";
}

# Colorize matches (basic implementation)
$text = "text with pattern in it";
if ($text =~ /pattern/) {
    print "$text\n";  # In a real implementation, you'd add ANSI color codes
} else {
    print "Color not supported\n";
}

# Quiet mode (exit status only, no output)
$has_pattern = 0;
open($fh, '<', "temp_file.txt") or die "Cannot read temp file: $!\n";
while (my $line = <$fh>) {
    if ($line =~ /pattern/) {
        $has_pattern = 1;
        last;
    }
}
close($fh);
if ($has_pattern) {
    print "found\n";
} else {
    print "not found\n";
}

# Cleanup
unlink("temp_file.txt");

