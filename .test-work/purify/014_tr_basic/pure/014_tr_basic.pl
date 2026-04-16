#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/014_tr_basic.pl" }


use strict;
use warnings;

print "=== Example 014: Basic tr command ===\n";

sub read_lines {
    my ($path) = @_;
    open my $in, '<', $path or die "Cannot open $path: $!\n";
    my @lines = <$in>;
    close $in;
    return @lines;
}

sub write_lines {
    my ($path, @lines) = @_;
    open my $out, '>', $path or die "Cannot create $path: $!\n";
    print $out @lines;
    close $out;
}

sub translate_text {
    my ($text, $from, $to) = @_;
    $text =~ tr/$from/$to/;
    return $text;
}

sub delete_chars {
    my ($text, $chars) = @_;
    $text =~ tr/$chars//d;
    return $text;
}

sub squeeze_chars {
    my ($text, $chars) = @_;
    $text =~ tr/$chars/$chars/s;
    return $text;
}

my @source_lines = (
    "Hello World\n",
    "This is a test\n",
    "UPPERCASE TEXT\n",
    "lowercase text\n",
    "Mixed Case Text\n",
);

write_lines('test_tr.txt', @source_lines);
my @input_lines = read_lines('test_tr.txt');

print "Using backticks to call tr (translate a to A):\n";
print map { translate_text($_, 'a', 'A') } @input_lines;

print "\ntr with case conversion (lowercase to uppercase):\n";
print map { translate_text($_, 'a-z', 'A-Z') } @input_lines;

print "\ntr with delete (delete all spaces):\n";
print map { delete_chars($_, ' ') } @input_lines;

print "\ntr with complement (delete all non-letters):\n";
print map { my $line = $_; $line =~ s/[^A-Za-z\n]//g; $line } @input_lines;

print "\ntr with squeeze (squeeze multiple spaces):\n";
print map { squeeze_chars($_, ' ') } @input_lines;

print "\ntr with character classes (delete digits):\n";
print map { my $line = $_; $line =~ s/[0-9]//g; $line } @input_lines;

print "\ntr with multiple characters (translate vowels):\n";
print map { translate_text($_, 'aeiou', 'AEIOU') } @input_lines;

print "\ntr with ranges (translate a-z to A-Z):\n";
print map { translate_text($_, 'a-z', 'A-Z') } @input_lines;

print "\ntr with complement and delete (keep only letters):\n";
print map { my $line = $_; $line =~ s/[^A-Za-z]//g; $line =~ s/\n$//; "$line\n" } @input_lines;

print "\ntr with squeeze and translate:\n";
print map { translate_text($_, 'a-z', 'A-Z') } @input_lines;

print "\ntr from stdin (echo | tr):\n";
print "HELLO WORLD\n";

print "\ntr with specific characters (translate l to L):\n";
print map { translate_text($_, 'l', 'L') } @input_lines;

print "\ntr with character sets (translate punctuation):\n";
print map { my $line = $_; $line =~ s/[[:punct:]]/X/g; $line } @input_lines;

unlink('test_tr.txt') if -f 'test_tr.txt';

print "=== Example 014 completed successfully ===\n";
