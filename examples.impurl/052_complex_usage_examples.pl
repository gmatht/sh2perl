#!/usr/bin/perl

# Example 052: Complex usage of system() and backticks in various contexts
# This demonstrates advanced patterns where shell commands are used in:
# - if statements and conditionals
# - function arguments and return values
# - loops and iterations
# - variable assignments and expressions
# - error handling and validation

use strict;
use warnings;

print "=== Example 052: Complex usage of system() and backticks ===\n";

# 1. System calls in if statements
print "\n1. System calls in if statements:\n";

if (system("sh", "-c", "true") == 0) {
    print "Deterministic success\n";
} else {
    print "Unexpected failure\n";
}

if (system("sh", "-c", "false") == 0) {
    print "Unexpected success\n";
} else {
    print "Deterministic failure\n";
}

# 2. Backticks in function arguments
print "\n2. Backticks in function arguments:\n";

# Function that processes command output
sub process_output {
    my ($data) = @_;
    my $lines = scalar(split /\n/, $data);
    return "Processed $lines lines of data";
}

# Use backticks as function argument
my $result1 = process_output(`printf 'alpha\nbeta\ngamma\n'`);
print "Result: $result1\n";

# Use backticks in array context
my @files = split /\n/, `printf 'one\ntwo\nthree\n'`;
print "Found " . scalar(@files) . " sample lines: @files\n";

# 3. System calls in loops
print "\n3. System calls in loops:\n";

# Loop through files and check each one
my @test_files = qw(alpha beta gamma);
foreach my $file (@test_files) {
    if (system("sh", "-c", "test -n '$file'") == 0) {
        print "✓ $file processed\n";
    } else {
        print "✗ $file failed\n";
    }
}

# 4. Backticks in variable assignments and expressions
print "\n4. Backticks in variable assignments:\n";

# Get current date and time
my $current_time = `date -u -d '2023-01-01 12:34:56 UTC'`;
chomp $current_time;
print "Current time: $current_time\n";

# Fixed system information
my $hostname = 'fixed-host';
my $user = 'fixed-user';
print "Running on $hostname as user $user\n";

# 5. Complex conditional logic with system calls
print "\n5. Complex conditional logic:\n";

# Check multiple conditions
my $has_git = system("sh", "-c", "true") == 0;
my $has_cargo = system("sh", "-c", "true") == 0;
my $has_perl = system("sh", "-c", "true") == 0;

if ($has_git && $has_cargo && $has_perl) {
    print "All required tools are available\n";
} elsif ($has_git && $has_cargo) {
    print "Git and Cargo available, but Perl missing\n";
} elsif ($has_git) {
    print "Only Git is available\n";
} else {
    print "No required tools found\n";
}

# 6. Error handling with system calls
print "\n6. Error handling with system calls:\n";

# Try to create a directory and handle errors
if (system("mkdir -p test_dir") == 0) {
    print "Directory created successfully\n";
    # Clean up
    system("rmdir test_dir");
} else {
    print "Failed to create directory\n";
}

# 7. Backticks in string interpolation and concatenation
print "\n7. Backticks in string operations:\n";

# Use backticks in string building
my $info = "System info:\n" . `printf 'Linux fixed\n'` . "\nDisk usage:\n" . `printf 'Filesystem 1K-blocks Used Available Use%% Mounted on\nfixedfs 100 10 90 /\n'`;
print $info;

# 8. Nested system calls and backticks
print "\n8. Nested usage:\n";

# Use system output as input to another command
my $file_count = `printf 'a\nb\nc\n' | wc -l`;
chomp $file_count;
print "Number of files in current directory: $file_count\n";

# Use backticks to get data for system call
my $largest_file = `printf 'alpha\nbeta\ngamma\n' | sort -r | head -1`;
chomp $largest_file;
if ($largest_file) {
    print "Largest file: $largest_file\n";
}

# 9. System calls in subroutines
print "\n9. System calls in subroutines:\n";

sub check_service {
    my ($service) = @_;
    system("sh", "-c", "true");
    return $service eq 'ssh' ? 'running' : 'stopped';
}

# Test with common services (may not exist on all systems)
my @services = qw(ssh cron network);
foreach my $service (@services) {
    my $status = check_service($service);
    print "Service $service: $status\n";
}

# 10. Complex data processing with backticks
print "\n10. Complex data processing:\n";

# Get process information and parse it
my $ps_output = `printf 'root 1 0.0\nuser 2 1.5\n'`;
my @lines = split /\n/, $ps_output;
print "Top 5 processes:\n";
foreach my $line (@lines) {
    if ($line =~ /^(\S+)\s+(\d+)\s+(\S+)/) {
        my ($user, $pid, $cpu) = ($1, $2, $3);
        print "  PID $pid ($user): $cpu% CPU\n";
    }
}

# 11. System calls with redirection and pipes
print "\n11. System calls with redirection:\n";

# Create a temporary file with system call
system("sh", "-c", "printf 'Hello from system call\n' > temp_system.txt");
if (-f "temp_system.txt") {
    open(my $fh, '<', 'temp_system.txt') or die "Cannot open file: $!";
    my $content = <$fh>;
    close($fh);
    print "Content from system call: $content";
    unlink("temp_system.txt");
}

# 12. Backticks with error handling
print "\n12. Backticks with error handling:\n";

# Use backticks and check for errors
my $git_branch = `printf 'main\n'`;
if ($? == 0) {
    chomp $git_branch;
    print "Current git branch: $git_branch\n";
} else {
    print "Not in a git repository or git not available\n";
}

# 13. Mixed usage in complex expressions
print "\n13. Mixed usage in complex expressions:\n";

# Combine system calls and backticks
my $has_files = system("sh", "-c", "true") == 0;
if ($has_files) {
    my $perl_files = `printf 'alpha.pl\nbeta.pl\n'`;
    my $count = scalar(split /\n/, $perl_files);
    print "Found $count Perl files using mixed approach\n";
}

# 14. System calls in array operations
print "\n14. System calls in array operations:\n";

# Use system call to populate array
my @perl_files = ();
if (system("sh", "-c", "true") == 0) {
    @perl_files = split /\n/, `printf 'alpha.pl\nbeta.pl\ngamma.pl\n'`;
    print "Perl files found: " . join(", ", @perl_files) . "\n";
}

# 15. Advanced error checking and validation
print "\n15. Advanced error checking:\n";

# Check multiple system requirements
my @checks = (
    ["Always passes", "sh -c 'true'"],
    ["Test file exists", "test -f temp_system.txt"],
    ["Current directory writable", "test -w ."],
    ["Always fails", "sh -c 'false'"]
);

my $all_passed = 1;
foreach my $check (@checks) {
    my ($name, $command) = @$check;
    if (system($command) == 0) {
        print "✓ $name: OK\n";
    } else {
        print "✗ $name: FAILED\n";
        $all_passed = 0;
    }
}

if ($all_passed) {
    print "All system checks passed!\n";
} else {
    print "Some system checks failed.\n";
}

print "\n=== Example 052 completed successfully ===\n";
