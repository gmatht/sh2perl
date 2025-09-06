#!/usr/bin/perl

# Example 030: Basic tee command using system() and backticks
# This demonstrates the tee builtin called from Perl

print "=== Example 030: Basic tee command ===\n";

# Simple tee using backticks
print "Using backticks to call tee (write to file and stdout):\n";
my $tee_output = `echo "This is a test line" | tee test_tee_output.txt`;
print "Output: $tee_output";

# Check if file was created
if (-f "test_tee_output.txt") {
    print "File created successfully\n";
    my $file_content = `cat test_tee_output.txt`;
    print "File content: $file_content";
}

# tee with append using system()
print "\ntee with append (-a):\n";
system("echo", "This is another line", "|", "tee", "-a", "test_tee_output.txt");

# tee with multiple files using backticks
print "\ntee with multiple files:\n";
my $tee_multi = `echo "Line for multiple files" | tee test_tee1.txt test_tee2.txt test_tee3.txt`;
print "Output: $tee_multi";

# Check if multiple files were created
if (-f "test_tee1.txt" && -f "test_tee2.txt" && -f "test_tee3.txt") {
    print "Multiple files created successfully\n";
}

# tee with ignore interrupts using system()
print "\ntee with ignore interrupts (-i):\n";
system("echo", "This line ignores interrupts", "|", "tee", "-i", "test_tee_interrupt.txt");

# tee with pipe fail using backticks
print "\ntee with pipe fail (-p):\n";
my $tee_pipe = `echo "This line has pipe fail" | tee -p test_tee_pipe.txt`;
print "Output: $tee_pipe";

# tee with append and multiple files using system()
print "\ntee with append and multiple files:\n";
system("echo", "Appended line", "|", "tee", "-a", "test_tee1.txt", "test_tee2.txt");

# tee with output to stderr using backticks
print "\ntee with output to stderr:\n";
my $tee_stderr = `echo "This goes to stderr" | tee /dev/stderr`;
print "Output: $tee_stderr";

# tee with null output using system()
print "\ntee with null output:\n";
system("echo", "This goes to null", "|", "tee", "/dev/null");

# tee with multiple outputs using backticks
print "\ntee with multiple outputs:\n";
my $tee_multi_out = `echo "Multiple outputs" | tee test_tee_multi1.txt test_tee_multi2.txt /dev/stdout`;
print "Output: $tee_multi_out";

# tee with append and ignore interrupts using system()
print "\ntee with append and ignore interrupts:\n";
system("echo", "Appended with ignore interrupts", "|", "tee", "-a", "-i", "test_tee_append_interrupt.txt");

# tee with pipe fail and multiple files using backticks
print "\ntee with pipe fail and multiple files:\n";
my $tee_pipe_multi = `echo "Pipe fail with multiple files" | tee -p test_tee_pipe1.txt test_tee_pipe2.txt`;
print "Output: $tee_pipe_multi";

# Clean up
unlink('test_tee_output.txt') if -f 'test_tee_output.txt';
unlink('test_tee1.txt') if -f 'test_tee1.txt';
unlink('test_tee2.txt') if -f 'test_tee2.txt';
unlink('test_tee3.txt') if -f 'test_tee3.txt';
unlink('test_tee_interrupt.txt') if -f 'test_tee_interrupt.txt';
unlink('test_tee_pipe.txt') if -f 'test_tee_pipe.txt';
unlink('test_tee_multi1.txt') if -f 'test_tee_multi1.txt';
unlink('test_tee_multi2.txt') if -f 'test_tee_multi2.txt';
unlink('test_tee_append_interrupt.txt') if -f 'test_tee_append_interrupt.txt';
unlink('test_tee_pipe1.txt') if -f 'test_tee_pipe1.txt';
unlink('test_tee_pipe2.txt') if -f 'test_tee_pipe2.txt';

print "=== Example 030 completed successfully ===\n";
