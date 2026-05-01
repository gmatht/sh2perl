#!/usr/bin/perl

# Example 001: Basic echo command using system() and backticks
# This demonstrates the echo builtin called from Perl

print "=== Example 001: Basic echo command ===\n";

# Simple echo command using system()
print "Using system() to call echo:\n";
system("echo", "Hello, World!");

# Echo with multiple arguments using system()
print "\nEcho with multiple arguments:\n";
system("echo", "This is a test of the echo builtin");

# Echo with special characters using backticks
print "\nEcho with special characters using backticks:\n";
my $output = `printf '%s\\n' "Line 1" "Line 2" "Line 3"`;
print $output;

# Echo with variables using backticks
my $name = "Perl";
my $version = "5.32";
print "\nEcho with variables:\n";
my $var_output = `printf '%s\\n' "Welcome to $name version $version"`;
print $var_output;

# Demonstrate echo -e behavior vs printf for portability
print "\nEcho with echo -e (may be shell-dependent):\n";
my $echo_e = `sh -c 'echo -e "A\\nB"'`;
print $echo_e;

print "\nPrintf equivalent (portable):\n";
my $printf_out = `printf '%s\\n' A B`;
print $printf_out;

# Echo with quotes using system()
print "\nEcho with quotes:\n";
system("echo", "This is a 'quoted' string");
system("echo", 'This is a "double quoted" string');

# Echo with redirection using system()
print "\nEcho with redirection:\n";
system("echo 'Redirected output' > temp_echo.txt");
if (-f "temp_echo.txt") {
    open(my $fh, '<', 'temp_echo.txt') or die "Cannot open file: $!";
    my $content = <$fh>;
    close($fh);
    print "File content: $content";
    unlink("temp_echo.txt");
}

print "\n=== Example 001 completed successfully ===\n";
