#!/usr/bin/perl

# Example 036: Basic control flow using system() and backticks
# This demonstrates control flow with builtins called from Perl

print "=== Example 036: Basic control flow ===\n";

# Create test files first
open(my $fh, '>', 'test_control.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
close($fh);

# if-then-else with builtins using system()
print "Using system() for if-then-else with builtins:\n";
if (-f "test_control.txt") {
    print "File exists, counting lines:\n";
    system("wc", "-l", "test_control.txt");
} else {
    print "File does not exist, creating it:\n";
    system("touch", "test_control.txt");
}

# while loop with builtins using backticks
print "\nUsing backticks for while loop with builtins:\n";
my $counter = 0;
while ($counter < 3) {
    
    print $output;
    $counter++;
}

# for loop with builtins using system()
print "\nUsing system() for for loop with builtins:\n";
for my $i (1..3) {
    print "Loop iteration $i:\n";
    system("echo", "Processing item $i");
}

# case statement simulation with builtins using backticks
print "\nUsing backticks for case statement simulation:\n";
my $file_type = "txt";
if ($file_type eq "txt") {
    
    print "File type: $output";
} elsif ($file_type eq "pl") {
    
    print "File type: $output";
} else {
    
    print "File type: $output";
}

# nested if with builtins using system()
print "\nUsing system() for nested if with builtins:\n";
if (-f "test_control.txt") {
    if (-s "test_control.txt") {
        print "File exists and has content:\n";
        system("cat", "test_control.txt");
    } else {
        print "File exists but is empty\n";
    }
} else {
    print "File does not exist\n";
}

# function simulation with builtins using backticks
print "\nUsing backticks for function simulation:\n";
sub process_file {
    my $filename = shift;
    if (-f $filename) {
        
        
        
        return ($lines, $words, $chars);
    }
    return (0, 0, 0);
}

my ($lines, $words, $chars) = process_file("test_control.txt");
print "Lines: $lines";
print "Words: $words";
print "Chars: $chars";

# error handling with builtins using system()
print "\nUsing system() for error handling with builtins:\n";
my $result = system("grep", "nonexistent", "test_control.txt");
if ($result == 0) {
    print "Pattern found\n";
} else {
    print "Pattern not found\n";
}

# conditional execution with builtins using backticks
print "\nUsing backticks for conditional execution:\n";

print $file_exists;

# loop with break condition using system()
print "\nUsing system() for loop with break condition:\n";
my $found = 0;
for my $i (1..5) {
    print "Checking iteration $i:\n";
    my $result = system("grep", "line", "test_control.txt");
    if ($result == 0) {
        print "Pattern found, breaking\n";
        $found = 1;
        last;
    }
}

# Clean up
unlink('test_control.txt') if -f 'test_control.txt';

print "=== Example 036 completed successfully ===\n";
