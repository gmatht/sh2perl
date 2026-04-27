sub __bt { my $s = join('', @_); wantarray ? (split /^/, $s, -1) : $s }
#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/006_head_basic.pl" }


print "=== Example 006: Basic head command ===\n";

open(my $fh, '>', 'test_head.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

print "Using backticks to call head (default 10 lines):\n";
my $head_output = __bt(do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head test_head.txt'; qx{$head_cmd}; }
);
print $head_output;

print "\nhead -n 5 (first 5 lines):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('head', '-n', '5', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nhead -n 3 (first 3 lines):\n";
my $head3 = __bt(do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -n 3 test_head.txt'; qx{$head_cmd}; }
);
print $head3;

print "\nhead -n 1 (first line only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('head', '-n', '1', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nhead -n 15 (more than available):\n";
my $head15 = __bt(do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -n 15 test_head.txt'; qx{$head_cmd}; }
);
print $head15;

print "\nhead -c 50 (first 50 characters):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('head', '-c', '50', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nhead -c 100 (first 100 characters):\n";
my $head_bytes = __bt(do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -c 100 test_head.txt'; qx{$head_cmd}; }
);
print $head_bytes;

print "\nhead from stdin (echo | head):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', q{echo 'Line 1
Line 2
Line 3
Line 4
Line 5' | head -n 3}); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nhead -q (quiet mode, no filename):\n";
my $head_quiet = __bt(do { my $output_0 = q{}; my $output_printed_0; my $head_cmd = 'head -q test_head.txt'; qx{$head_cmd}; }
);
print $head_quiet;

print "\nhead -v (verbose mode, with filename):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('head', '-v', 'test_head.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

unlink('test_head.txt') if -f 'test_head.txt';

print "=== Example 006 completed successfully ===\n";
