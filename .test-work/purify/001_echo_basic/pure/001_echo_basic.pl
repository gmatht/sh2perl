#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/001_echo_basic.pl" }


print "=== Example 001: Basic echo command ===\n";

print "Using " . "sys" . "tem" . "() to call echo:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "Hello, World!"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nEcho with multiple arguments:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "This is a test of the echo builtin"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nEcho with special characters using backticks:\n";
my $output = ("Line 1
Line 2
Line 3") . "\n"
;
print $output;

my $name = "Perl";
my $version = "5.32";
print "\nEcho with variables:\n";
my $var_output = ("Welcome to $name version $version") . "\n"
;
print $var_output;

print "\nEcho with quotes:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "This is a 'quoted' string"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", 'This is a "double quoted" string'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nEcho with redirection:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_echo.txt'
      or die "Cannot open file: $!\n";
    print 'Redirected output' . "\n";
    $CHILD_ERROR = 0;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
if (-f "temp_echo.txt") {
    open(my $fh, '<', 'temp_echo.txt') or die "Cannot open file: $!";
    my $content = <$fh>;
    close($fh);
    print "File content: $content";
    unlink("temp_echo.txt");
}

print "\n=== Example 001 completed successfully ===\n";
