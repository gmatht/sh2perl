sub __bt { my $s = join('', @_); wantarray ? (split /^/, $s, -1) : $s }
#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/027_basename_basic.pl" }


print "=== Example 027: Basic basename command ===\n";

print "Using backticks to call basename:\n";
my $basename_output = __bt(do { my $basename_cmd = 'basename /path/to/file.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "basename /path/to/file.txt: $basename_output";

print "\nbasename with suffix (remove .txt):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '/path/to/file.txt', '.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with multiple suffixes:\n";
my $basename_multi = __bt(do { my $basename_cmd = 'basename /path/to/file.txt .txt .bak'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "basename /path/to/file.txt .txt .bak: $basename_multi";

print "\nbasename with zero suffix (-s ''):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '-s', '', '/path/to/file.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with multiple paths:\n";
my $basename_paths = __bt(do { my $basename_cmd = 'basename /path/to/file1.txt /path/to/file2.txt /path/to/file3.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "Multiple paths: $basename_paths";

print "\nbasename with directory:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '/home/user/documents/'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with current directory:\n";
my $basename_current = __bt(do { my $basename_cmd = 'basename .'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "Current directory: $basename_current";

print "\nbasename with parent directory:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '..'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with root directory:\n";
my $basename_root = __bt(do { my $basename_cmd = 'basename /'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "Root directory: $basename_root";

print "\nbasename with empty string:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', ''); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with relative path:\n";
my $basename_relative = __bt(do { my $basename_cmd = 'basename ../file.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "Relative path: $basename_relative";

print "\nbasename with hidden file:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '/path/to/.hidden.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nbasename with file without extension:\n";
my $basename_no_ext = __bt(do { my $basename_cmd = 'basename /path/to/file'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
);
print "File without extension: $basename_no_ext";

print "\nbasename with multiple extensions:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('basename', '/path/to/file.txt.bak', '.txt.bak'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "=== Example 027 completed successfully ===\n";
