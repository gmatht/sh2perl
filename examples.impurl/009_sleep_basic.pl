#!/usr/bin/perl

# Example 009: Basic sleep command using system() and backticks
# This demonstrates the sleep builtin called from Perl

print "=== Example 009: Basic sleep command ===\n";

# Simple sleep command using system()
print "Using system() to call sleep (2 seconds):\n";
my $start_time = time();
system("sleep", "2");
my $end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

# sleep with fractional seconds using backticks
print "\nUsing backticks to call sleep (1.5 seconds):\n";
$start_time = time();
my $sleep_output = `sleep 1.5`;
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

# sleep with different durations using system()
print "\nSleep with different durations:\n";
for my $duration (1, 2, 3) {
    print "Sleeping for $duration second(s)...\n";
    $start_time = time();
    system("sleep", $duration);
    $end_time = time();
    print "Actually slept for " . ($end_time - $start_time) . " seconds\n";
}

# sleep with fractional durations using backticks
print "\nSleep with fractional durations:\n";
my @durations = (0.5, 1.0, 1.5);
foreach my $duration (@durations) {
    print "Sleeping for $duration second(s)...\n";
    $start_time = time();
    my $result = `sleep $duration`;
    $end_time = time();
    print "Actually slept for " . ($end_time - $start_time) . " seconds\n";
}

# sleep with very short duration using system()
print "\nSleep with very short duration (0.1 seconds):\n";
$start_time = time();
system("sleep", "0.1");
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

# sleep with long duration using backticks (but short for demo)
print "\nSleep with longer duration (3 seconds):\n";
$start_time = time();
my $long_sleep = `sleep 3`;
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

# sleep with multiple arguments using system()
print "\nSleep with multiple arguments (sleep 1 2 3):\n";
$start_time = time();
system("sleep", "1", "2", "3");  # This will sleep for 6 seconds total
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "=== Example 009 completed successfully ===\n";
