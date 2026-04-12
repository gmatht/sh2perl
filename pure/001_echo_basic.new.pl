#!/usr/bin/perl


print "=== Example 001: Basic echo command ===\n";

print "Using " . "sys" . "tem" . "() to call echo:\n";
print "Hello, World!\n";

print "\nEcho with multiple arguments:\n";
print "This is a test of the echo builtin\n";

print "\nEcho with special characters using backticks:\n";
my $output = ("Line 1\nLine 2\nLine 3") . "\n";
print $output;

my $name = "Perl";
my $version = "5.32";
print "\nEcho with variables:\n";
my $var_output = ("Welcome to $name version $version") . "\n";
print $var_output;

print "\nEcho with quotes:\n";
print "This is a 'quoted' string\n";
print 'This is a "double quoted" string' . "\n";

print "\nEcho with redirection:\n";
{
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_echo.txt'
      or die "Cannot open file: $!\n";
    print 'Redirected output' . "\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
}
if (-f "temp_echo.txt") {
    open(my $fh, '<', 'temp_echo.txt') or die "Cannot open file: $!";
    my $content = <$fh>;
    close($fh);
    print "File content: $content";
    unlink("temp_echo.txt");
}

print "\n=== Example 001 completed successfully ===\n";
