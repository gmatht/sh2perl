#!/usr/bin/perl

# Example 034: Basic nohup command using system() and backticks
# This demonstrates the nohup builtin called from Perl

print "=== Example 034: Basic nohup command ===\n";

# Simple nohup using backticks
print "Using backticks to call nohup (nohup echo):\n";
my $nohup_output = `nohup echo "This is a nohup command" 2>&1`;
print $nohup_output;

# nohup with sleep command using system()
print "\nnohup with sleep command:\n";
system("nohup", "sleep", "2", "&");

# nohup with output redirection using backticks
print "\nnohup with output redirection:\n";
my $nohup_redirect = `nohup echo "Output to file" > nohup_test.txt 2>&1 &`;
print "Background process started\n";

# Check if output file was created
if (-f "nohup_test.txt") {
    print "Output file created successfully\n";
    my $file_content = `cat nohup_test.txt`;
    print "File content: $file_content";
}

# nohup with different command using system()
print "\nnohup with different command:\n";
system("nohup", "ls", "-la", ">", "nohup_ls.txt", "2>&1", "&");

# Check if ls output file was created
if (-f "nohup_ls.txt") {
    print "LS output file created\n";
    my $ls_content = `cat nohup_ls.txt`;
    print "LS content: $ls_content";
}

# nohup with error handling using backticks
print "\nnohup with error handling:\n";
my $nohup_error = `nohup nonexistent_command 2>&1`;
print "Error result: $nohup_error";

# nohup with pipe using system()
print "\nnohup with pipe:\n";
system("nohup", "echo", "Hello World", "|", "cat", "2>&1");

# nohup with background and output using backticks
print "\nnohup with background and output:\n";
my $nohup_bg = `nohup echo "Background process" > nohup_bg.txt 2>&1 &`;
print "Background process with output started\n";

# nohup with multiple commands using system()
print "\nnohup with multiple commands:\n";
system("nohup", "sh", "-c", "echo 'Command 1'; echo 'Command 2'", ">", "nohup_multi.txt", "2>&1", "&");

# nohup with environment variables using backticks
print "\nnohup with environment variables:\n";
my $nohup_env = `nohup env | head -3 2>&1`;
print $nohup_env;

# nohup with different working directory using system()
print "\nnohup with different working directory:\n";
system("nohup", "pwd", ">", "nohup_pwd.txt", "2>&1");

# nohup with signal handling using backticks
print "\nnohup with signal handling:\n";
my $nohup_signal = `nohup sleep 5 2>&1 &`;
print "Background sleep process started\n";

# nohup with output to null using system()
print "\nnohup with output to null:\n";
system("nohup", "echo", "This goes to null", ">", "/dev/null", "2>&1");

# Clean up
unlink('nohup_test.txt') if -f 'nohup_test.txt';
unlink('nohup_ls.txt') if -f 'nohup_ls.txt';
unlink('nohup_bg.txt') if -f 'nohup_bg.txt';
unlink('nohup_multi.txt') if -f 'nohup_multi.txt';
unlink('nohup_pwd.txt') if -f 'nohup_pwd.txt';

print "=== Example 034 completed successfully ===\n";
