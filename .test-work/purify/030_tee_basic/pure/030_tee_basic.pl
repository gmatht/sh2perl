#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/030_tee_basic.pl" }


print "=== Example 030: Basic tee command ===\n";

print "Using backticks to call tee (write to file and stdout):\n";
my $tee_output = do { my $pipeline_cmd = 'echo This is a test line | tee test_tee_output.txt'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_output";

if (-f "test_tee_output.txt") {
    print "File created successfully\n";
    my $file_content = do { open my $fh, '<', 'test_tee_output.txt' or die 'cat: ' . 'test_tee_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "File content: $file_content";
}

print "\ntee with append (-a):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "This is another line", "|", "tee", "-a", "test_tee_output.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\ntee with multiple files:\n";
my $tee_multi = do { my $pipeline_cmd = 'echo Line for multiple files | tee test_tee1.txt test_tee2.txt test_tee3.txt'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_multi";

if (-f "test_tee1.txt" && -f "test_tee2.txt" && -f "test_tee3.txt") {
    print "Multiple files created successfully\n";
}

print "\ntee with ignore interrupts (-i):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "This line ignores interrupts", "|", "tee", "-i", "test_tee_interrupt.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\ntee with pipe fail (-p):\n";
my $tee_pipe = do { my $pipeline_cmd = 'echo This line has pipe fail | tee -p test_tee_pipe.txt'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_pipe";

print "\ntee with append and multiple files:\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "Appended line", "|", "tee", "-a", "test_tee1.txt", "test_tee2.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\ntee with output to stderr:\n";
my $tee_stderr = do { my $pipeline_cmd = 'echo This goes to stderr | tee /dev/stderr'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_stderr";

print "\ntee with null output:\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "This goes to null", "|", "tee", "/dev/null"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\ntee with multiple outputs:\n";
my $tee_multi_out = do { my $pipeline_cmd = 'echo Multiple outputs | tee test_tee_multi1.txt test_tee_multi2.txt /dev/stdout'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_multi_out";

print "\ntee with append and ignore interrupts:\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("echo", "Appended with ignore interrupts", "|", "tee", "-a", "-i", "test_tee_append_interrupt.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\ntee with pipe fail and multiple files:\n";
my $tee_pipe_multi = do { my $pipeline_cmd = 'echo Pipe fail with multiple files | tee -p test_tee_pipe1.txt test_tee_pipe2.txt'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Output: $tee_pipe_multi";

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
