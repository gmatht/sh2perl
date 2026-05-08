#!/usr/bin/perl
BEGIN { $0 = "examples.impurl/044_yes_command.pl" }


print "=== Example 044: yes command ===\n";

print "Using backticks to call yes (limited output):\n";
my $yes_output = do { my $pipeline_cmd = q{yes 'Hello World' | head -5}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_output;

print "\nyes with specific string:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"'Test String'"'"' | head -3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with default string:\n";
my $yes_default = do { my $pipeline_cmd = 'yes | head -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_default;

print "\nyes with empty string:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"''"'"' | head -3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with special characters:\n";
my $yes_special = do { my $pipeline_cmd = q{yes '!@#$%^&*()' | head -3}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_special;

print "\nyes with numbers:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes 12345 | head -3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with newlines:\n";
my $yes_newlines = do { my $pipeline_cmd = q{yes 'Line with\nnewline' | head -3}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_newlines;

print "\nyes with pipe to other commands:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes test | grep test | head -3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with pipe to other commands:\n";
my $yes_pipe = do { my $pipeline_cmd = 'yes data | tr a-z A-Z | head -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_pipe;

print "\nyes with output redirection:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"'Output to file'"'"' | head -5 > yes_output.txt'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

if (-f "yes_output.txt") {
    print "Output file created successfully\n";
    my $file_content = do { open my $fh, '<', 'yes_output.txt' or die 'cat: ' . 'yes_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "File content:\n$file_content";
}

print "\nyes with background process (bounded):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"'Background'"'"' | head -n 3 > /dev/null &'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};
print "Background process started (will exit shortly)\n";

print "\nyes with timeout (bounded):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"'Timeout test'"'"' | head -n 3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with different strings:\n";
my $yes_diff = do { my $pipeline_cmd = q{yes 'String 1' | head -2}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_diff;
my $yes_diff2 = do { my $pipeline_cmd = q{yes 'String 2' | head -2}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_diff2;

print "\nyes with error handling:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("sh", "-c", q{'yes '"'"'Error test'"'"' 2>/dev/null | head -3'}); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\nyes with pipe to wc:\n";
my $yes_wc = do { my $pipeline_cmd = q{yes 'Count me' | head -10 | wc -l}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Count: $yes_wc";

unlink('yes_output.txt') if -f 'yes_output.txt';

print "=== Example 044 completed successfully ===\n";
