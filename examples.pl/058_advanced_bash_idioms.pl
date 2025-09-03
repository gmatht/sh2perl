#!/usr/bin/env perl

use strict;
use warnings;

# Advanced Bash Idioms: Nesting and Combining Control Blocks
# This file demonstrates complex Perl patterns and idioms

print "=== Advanced Bash Idioms Examples ===\n";
print "\n";

# Example 1: Nested loops with conditional logic and array manipulation
print "1. Nested loops with conditional logic and array manipulation:\n";
my @numbers = (1, 2, 3, 4, 5);
my @letters = qw(a b c d e);
for my $num (@numbers) {
    for my $letter (@letters) {
        if ($num > 3 && $letter ne "c") {
            print "  Number $num with letter $letter (filtered)\n";
        }
    }
}
print "\n";

# Example 2: Function with nested case statements and parameter expansion
print "2. Function with nested case statements and parameter expansion:\n";
sub process_data {
    my ($data_type, $value) = @_;
    
    if ($data_type eq "string") {
        my $lower_value = lc($value);
        if ($lower_value =~ /^(hello|hi)$/) {
            print "  Greeting detected: $value\n";
        } elsif ($lower_value =~ /^(bye|goodbye)$/) {
            print "  Farewell detected: $value\n";
        } else {
            print "  Unknown string: $value\n";
        }
    } elsif ($data_type eq "number") {
        if ($value =~ /^\d+$/) {
            if ($value % 2 == 0) {
                print "  Even number: $value\n";
            } else {
                print "  Odd number: $value\n";
            }
        } else {
            print "  Invalid number: $value\n";
        }
    } else {
        print "  Unknown data type: $data_type\n";
    }
}

process_data("string", "Hello");
process_data("string", "Bye");
process_data("number", "42");
process_data("number", "17");
print "\n";

# Example 3: Complex conditional with command substitution and arithmetic
print "3. Complex conditional with command substitution and arithmetic:\n";
my $file_count = 0;
my $dir_count = 0;
opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    next if $file =~ /^\.\.?$/;
    if (-f $file) {
        $file_count++;
    } elsif (-d $file) {
        $dir_count++;
    }
}
closedir($dh);

if ($file_count > 0 && $dir_count > 1) {
    if ($file_count > $dir_count) {
        print "  More files ($file_count) than directories ($dir_count)\n";
    } elsif ($file_count == $dir_count) {
        print "  Equal count: $file_count files and $dir_count directories\n";
    } else {
        print "  More directories ($dir_count) than files ($file_count)\n";
    }
} else {
    print "  Insufficient items for comparison\n";
}
print "\n";

# Example 4: Nested here-documents with parameter expansion
print "4. Nested here-documents with parameter expansion:\n";
my $user = "admin";
my $host = "localhost";
my $port = "22";

print "    SSH Configuration:\n";
print "        User: $user\n";
print "        Host: $host\n";
print "        Port: $port\n";
my $status = system("ping -c 1 $host >/dev/null 2>&1") == 0 ? "Online" : "Offline";
print "        Status: $status\n";
print "\n";

# Example 5: Array processing with nested loops and conditional logic
print "5. Array processing with nested loops and conditional logic:\n";
my %matrix;
$matrix{0,0} = 1; $matrix{0,1} = 2; $matrix{0,2} = 3;
$matrix{1,0} = 4; $matrix{1,1} = 5; $matrix{1,2} = 6;
$matrix{2,0} = 7; $matrix{2,1} = 8; $matrix{2,2} = 9;

for my $i (0..2) {
    for my $j (0..2) {
        my $value = $matrix{$i,$j};
        if ($value > 5) {
            print "  [$value] ";
        } else {
            print "  $value ";
        }
    }
    print "\n";
}
print "\n";

# Example 6: Process substitution with nested commands and error handling
print "6. Process substitution with nested commands and error handling:\n";
my $test_string = "hello world test";
print "  First word: " . (split(' ', $test_string))[0] . "\n";
print "  Last word: " . (split(' ', $test_string))[-1] . "\n";
print "  Middle: " . join(' ', @{[split(' ', $test_string)]}[1..-2]) . "\n";
print "  Middle: " . join(' ', @{[split(' ', $test_string)]}[0..-2]) . "\n";
print "  Uppercase: " . uc($test_string) . "\n";
print "  Lowercase: " . lc($test_string) . "\n";
print "  Capitalize: " . ucfirst($test_string) . "\n";
print "\n";

# Example 11: Complex arithmetic with nested expressions
print "11. Complex arithmetic with nested expressions:\n";
my $a = 10;
my $b = 5;
my $c = 3;

my $result = ($a + $b) * $c - int($a % $b / $c);
print "  Expression: (a + b) * c - (a % b) / c\n";
print "  Values: a=$a, b=$b, c=$c\n";
print "  Result: $result\n";

# Nested arithmetic in conditional
if (($a > $b) && (($b < $c) || ($a % 2 == 0))) {
    print "  Complex condition met: a > b AND (b < c OR a is even)\n";
}
print "\n";

# Example 12: Nested command substitution with error handling
print "12. Nested command substitution with error handling:\n";
print "  Current directory: " . `pwd` . "\n";
print "  Parent directory: " . `dirname \`pwd\`` . "\n";
print "  Home directory: " . `dirname \`dirname \`pwd\`\`` . "\n";

# Nested command with fallback
my $file_info = `stat -c "%s %y" "nonexistent_file" 2>/dev/null` || "File not found";
print "  File info: $file_info\n";
print "\n";

print "=== Advanced Bash Idioms Examples Complete ===\n";
