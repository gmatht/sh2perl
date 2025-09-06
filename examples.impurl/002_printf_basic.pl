#!/usr/bin/perl

# Example 002: Basic printf command using system() and backticks
# This demonstrates the printf builtin called from Perl

print "=== Example 002: Basic printf command ===\n";

# Simple printf command using system()
print "Using system() to call printf:\n";
system("printf", "Hello, %s!\n", "World");

# printf with multiple format specifiers using backticks
print "\nprintf with multiple format specifiers:\n";
my $output = `printf "Name: %s, Age: %d, Score: %.2f\n" "Alice" 25 95.5`;
print $output;

# printf with different format types using system()
print "\nprintf with different format types:\n";
system("printf", "Integer: %d\n", 42);
system("printf", "Float: %.2f\n", 3.14159);
system("printf", "String: %s\n", "test");
system("printf", "Character: %c\n", 65);  # ASCII 'A'
system("printf", "Hexadecimal: %x\n", 255);
system("printf", "Octal: %o\n", 64);

# printf with field width and padding using backticks
print "\nprintf with field width and padding:\n";
my $table_output = `printf "%-10s %5d\n" "Item1" 100`;
print $table_output;
$table_output = `printf "%-10s %5d\n" "Item2" 2000`;
print $table_output;
$table_output = `printf "%-10s %5d\n" "Item3" 30`;
print $table_output;

# printf with precision using system()
print "\nprintf with precision:\n";
system("printf", "%.3f\n", 3.14159265359);
system("printf", "%.2e\n", 1234567.89);

# printf with zero padding using backticks
print "\nprintf with zero padding:\n";
my $padded = `printf "%05d\n" 42`;
print $padded;
$padded = `printf "%08x\n" 255`;
print $padded;

# printf with variables using backticks
my $name = "Perl";
my $count = 42;
print "\nprintf with Perl variables:\n";
my $var_output = `printf "Variable: %s, Count: %d\n" "$name" $count`;
print $var_output;

print "=== Example 002 completed successfully ===\n";
