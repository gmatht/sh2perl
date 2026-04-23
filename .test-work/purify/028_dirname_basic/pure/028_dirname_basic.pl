#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/028_dirname_basic.pl" }


print "=== Example 028: Basic dirname command ===\n";

print "Using backticks to call dirname:\n";
my $dirname_output = do { my $dirname_cmd = 'dirname /path/to/file.txt'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "dirname /path/to/file.txt: $dirname_output";

print "\ndirname with multiple paths:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", "/path/to/file1.txt", "/path/to/file2.txt", "/path/to/file3.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with current directory:\n";
my $dirname_current = do { my $dirname_cmd = 'dirname .'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "Current directory: $dirname_current";

print "\ndirname with parent directory:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", ".."); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with root directory:\n";
my $dirname_root = do { my $dirname_cmd = 'dirname /'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "Root directory: $dirname_root";

print "\ndirname with empty string:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", ""); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with relative path:\n";
my $dirname_relative = do { my $dirname_cmd = 'dirname ../file.txt'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "Relative path: $dirname_relative";

print "\ndirname with hidden file:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", "/path/to/.hidden.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with file in root:\n";
my $dirname_root_file = do { my $dirname_cmd = 'dirname /file.txt'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "File in root: $dirname_root_file";

print "\ndirname with directory path:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", "/home/user/documents/"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with nested path:\n";
my $dirname_nested = do { my $dirname_cmd = 'dirname /a/b/c/d/e/file.txt'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "Nested path: $dirname_nested";

print "\ndirname with single level path:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", "/file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ndirname with multiple levels:\n";
my $dirname_multi = do { my $dirname_cmd = 'dirname /usr/local/bin/script.sh'; my $dirname_output = qx{$dirname_cmd}; $CHILD_ERROR = $? >> 8; $dirname_output; }
;
print "Multiple levels: $dirname_multi";

print "\ndirname with zero option (-z):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("dirname", "-z", "/path/to/file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "=== Example 028 completed successfully ===\n";
