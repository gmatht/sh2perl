#!/usr/bin/env perl

use strict;
use warnings;

print "Warmup 1\n";
print "Fruit: apple\n";

print "Warmup 2\n";
print "Shell variable: $ENV{SHELL_VAR}\n";

# Example 1: Simple Perl one-liner to print text
print "=== Example 1: Simple Perl one-liner ===\n";
print "Hello from Perl!\n";

# Example 2: Perl script with command line arguments
print "\n=== Example 2: Perl with arguments ===\n";
for my $arg (@ARGV) {
    print "Argument: $arg\n";
}

# Example 3: Perl script processing shell variables
print "\n=== Example 3: Perl processing shell variables ===\n";
$ENV{SHELL_VAR} = "Hello World";
#print "Shell variable: $ENV{SHELL_VAR}\n";
print 'Shell variable: $ENV{SHELL_VAR}\n';

# Example 4: Perl script reading from shell pipeline
print "\n=== Example 4: Perl reading from pipeline ===\n";
my @fruits = qw(apple banana cherry);
for my $fruit (@fruits) {
    print "Fruit: $fruit\n";
}

# Example 5: Complex Perl script with here document
print "\n=== Example 5: Perl script with here document ===\n";
my @numbers = (1, 2, 3, 4, 5);
my $sum = 0;

for my $num (@numbers) {
    $sum += $num;
    print "Added $num, sum is now $sum\n";
}

print "Final sum: $sum\n";

#PERL_MUST_NOT_CONTAIN: `
#PERL_MUST_NOT_CONTAIN: system
