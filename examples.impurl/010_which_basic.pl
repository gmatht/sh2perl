#!/usr/bin/perl

# Example 010: Basic which command using system() and backticks
# This demonstrates the which builtin called from Perl

print "=== Example 010: Basic which command ===\n";

# Simple which command using backticks
print "Using backticks to call which:\n";
my $which_output = `which ls`;
print "which ls: $which_output";

# which with multiple commands using system()
print "\nwhich with multiple commands:\n";
system("which", "ls", "cat", "grep");

# which with different commands using backticks
print "\nwhich with different commands:\n";
my @commands = qw(ls cat grep echo printf date sleep);
foreach my $cmd (@commands) {
    my $path = `which $cmd`;
    chomp $path;
    if ($path) {
        print "$cmd: $path\n";
    } else {
        print "$cmd: not found\n";
    }
}

# which with all matches using system()
print "\nwhich with all matches (-a):\n";
system("which", "-a", "ls");

# which with quiet mode using backticks
print "\nwhich with quiet mode (-q):\n";
my $quiet_result = `which -q ls`;
print "Exit code: $?\n";

# which with version using system()
print "\nwhich with version (-v):\n";
system("which", "-v", "ls");

# which with non-existent command using backticks
print "\nwhich with non-existent command:\n";
my $not_found = `which nonexistentcommand 2>/dev/null`;
if ($not_found) {
    print "Found: $not_found\n";
} else {
    print "Command not found\n";
}

# which with built-in commands using system()
print "\nwhich with built-in commands:\n";
my @builtins = qw(echo printf cd);
foreach my $builtin (@builtins) {
    print "Checking $builtin: ";
    system("which", $builtin);
}

# which with PATH modification using backticks
print "\nwhich with PATH modification:\n";
my $original_path = $ENV{PATH};
$ENV{PATH} = "/bin:/usr/bin";
my $path_result = `which ls`;
print "ls with modified PATH: $path_result";
$ENV{PATH} = $original_path;

print "=== Example 010 completed successfully ===\n";
